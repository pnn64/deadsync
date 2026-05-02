use chrono::{Datelike, Local, Timelike};
use image::image_dimensions;
use log::{debug, info};
use mlua::{Function, Lua, MultiValue, Table, Value, ffi};
use std::collections::{HashMap, HashSet};
use std::ffi::c_int;
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::engine::present::actors::{TextAlign, TextAttribute};
use crate::engine::present::anim::EffectClock;
#[cfg(test)]
use crate::engine::present::anim::EffectMode;

mod overlay;
mod types;
mod util;

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
pub use self::types::{
    CompiledSongLua, SongLuaCapturedActor, SongLuaCompileContext, SongLuaCompileInfo,
    SongLuaDifficulty, SongLuaEaseTarget, SongLuaEaseWindow, SongLuaMessageEvent, SongLuaModWindow,
    SongLuaPlayerContext, SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit,
};
use self::util::{
    create_color_constants_table, ease_window_cmp, file_path_string, is_song_lua_audio_path,
    is_song_lua_image_path, is_song_lua_media_path, is_song_lua_video_path, lua_format_text,
    lua_text_value, lua_values_equal, make_color_table, message_event_cmp, method_arg,
    mod_window_cmp, parse_color_text, player_index_from_value, player_number_name, read_boolish,
    read_color_call, read_color_value, read_easing_name, read_f32, read_i32_value, read_player,
    read_song_lua_sound_paths, read_span_mode, read_string, read_u32_value,
    read_vertex_colors_value, song_dir_string, song_group_name, song_music_path,
    song_named_image_path, song_simfile_path, truthy,
};

const LUA_PLAYERS: usize = 2;
const SONG_LUA_NOTE_COLUMNS: usize = 4;
const SONG_LUA_PRODUCT_FAMILY: &str = "ITGmania";
const SONG_LUA_PRODUCT_ID: &str = "ITGmania";
const SONG_LUA_PRODUCT_VERSION: &str = "1.2.0";
const THEME_RECEPTOR_Y_STD: f32 = -125.0;
const THEME_RECEPTOR_Y_REV: f32 = 145.0;
const SONG_LUA_COLUMN_X: [f32; SONG_LUA_NOTE_COLUMNS] = [-96.0, -32.0, 32.0, 96.0];
const SONG_LUA_RUNTIME_KEY: &str = "__songlua_compile_song_runtime";
const SONG_LUA_RUNTIME_BEAT_KEY: &str = "__songlua_song_beat";
const SONG_LUA_RUNTIME_SECONDS_KEY: &str = "__songlua_music_seconds";
const SONG_LUA_RUNTIME_DELTA_BEAT_KEY: &str = "__songlua_song_delta_beat";
const SONG_LUA_RUNTIME_DELTA_SECONDS_KEY: &str = "__songlua_music_delta_seconds";
const SONG_LUA_RUNTIME_BPS_KEY: &str = "__songlua_song_bps";
const SONG_LUA_RUNTIME_RATE_KEY: &str = "__songlua_music_rate";
const SONG_LUA_SIDE_EFFECT_COUNT_KEY: &str = "__songlua_side_effect_count";
const SONG_LUA_SOUND_PATHS_KEY: &str = "__songlua_sound_paths";
const SONG_LUA_BROADCASTS_KEY: &str = "__songlua_broadcast_messages";
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
const SONG_LUA_ACTIVE_COLOR_INDEX: i64 = 1;
const SL_COLORS: &[&str] = &[
    "#FF5D47", "#FF577E", "#FF47B3", "#DD57FF", "#8885ff", "#3D94FF", "#00B8CC", "#5CE087",
    "#AEFA44", "#FFFF00", "#FFBE00", "#FF7D00",
];
const SL_DECORATIVE_COLORS: &[&str] = &[
    "#FF3C23", "#FF003C", "#C1006F", "#8200A1", "#413AD0", "#0073FF", "#00ADC0", "#5CE087",
    "#AEFA44", "#FFFF00", "#FFBE00", "#FF7D00",
];
const ITG_DIFF_COLORS: &[&str] = &[
    "#a355b8", "#1ec51d", "#d6db41", "#ba3049", "#2691c5", "#F7F7F7",
];
const DDR_DIFF_COLORS: &[&str] = &[
    "#2dccef", "#eaa910", "#ff344d", "#30d81e", "#e900ff", "#F7F7F7",
];
const SL_JUDGMENT_COLORS: &[&str] = &[
    "#21CCE8", "#e29c18", "#66c955", "#b45cff", "#c9855e", "#ff3030",
];
const SL_FA_PLUS_COLORS: &[&str] = &[
    "#21CCE8", "#ffffff", "#e29c18", "#66c955", "#b45cff", "#ff3030", "#ff00cc",
];
const EASING_NAMES: &[&str] = &[
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

#[inline(always)]
fn song_display_bps(context: &SongLuaCompileContext) -> f32 {
    (context.song_display_bpms[0].max(context.song_display_bpms[1]) / 60.0).max(f32::EPSILON)
}

#[inline(always)]
fn song_music_rate(context: &SongLuaCompileContext) -> f32 {
    if context.song_music_rate.is_finite() && context.song_music_rate > 0.0 {
        context.song_music_rate
    } else {
        1.0
    }
}

#[inline(always)]
fn song_elapsed_seconds_for_beat(beat: f32, song_bps: f32, music_rate: f32) -> f32 {
    beat / (song_bps.max(f32::EPSILON) * music_rate.max(f32::EPSILON))
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

    out.beat_mods.extend(read_mod_windows(
        globals
            .get::<Option<Table>>("mods")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Beat,
    )?);
    out.time_mods.extend(read_mod_windows(
        globals
            .get::<Option<Table>>("mod_time")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Second,
    )?);
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

fn install_stdlib_compat(lua: &Lua, song_dir: &Path) -> mlua::Result<()> {
    let globals = lua.globals();
    let table: Table = globals.get("table")?;
    table.set(
        "getn",
        lua.create_function(|_, value: Value| {
            Ok(match value {
                Value::Table(table) => table.raw_len() as i64,
                _ => 0,
            })
        })?,
    )?;
    table.set(
        "rotate_right",
        lua.create_function(|lua, args: MultiValue| rotate_lua_table(lua, &args, false))?,
    )?;
    table.set(
        "rotate_left",
        lua.create_function(|lua, args: MultiValue| rotate_lua_table(lua, &args, true))?,
    )?;
    let string: Table = globals.get("string")?;
    if matches!(string.get::<Value>("gfind")?, Value::Nil) {
        let gmatch = string.get::<Value>("gmatch")?;
        string.set("gfind", gmatch)?;
    }
    string.set(
        "split",
        lua.create_function(|lua, args: MultiValue| {
            let text = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let separator = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            create_split_table(lua, &text, &separator)
        })?,
    )?;
    let math: Table = globals.get("math")?;
    if matches!(math.get::<Value>("round")?, Value::Nil) {
        math.set(
            "round",
            lua.create_function(|_, value: f64| {
                Ok(if value >= 0.0 {
                    (value + 0.5).floor()
                } else {
                    (value - 0.5).ceil()
                })
            })?,
        )?;
    }
    if matches!(math.get::<Value>("clamp")?, Value::Nil) {
        math.set(
            "clamp",
            lua.create_function(|_, (value, min, max): (f64, f64, f64)| Ok(value.clamp(min, max)))?,
        )?;
    }
    if matches!(math.get::<Value>("mod")?, Value::Nil) {
        math.set(
            "mod",
            lua.create_function(|_, (left, right): (f64, f64)| {
                Ok(if right == 0.0 { f64::NAN } else { left % right })
            })?,
        )?;
    }
    globals.set("unpack", table.get::<Value>("unpack")?)?;
    globals.set(
        "split",
        lua.create_function(|lua, args: MultiValue| {
            let separator = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let text = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            create_split_table(lua, &text, &separator)
        })?,
    )?;
    globals.set(
        "Basename",
        lua.create_function(|lua, value: Value| {
            Ok(Value::String(lua.create_string(path_basename(
                &read_string(value).unwrap_or_default(),
            ))?))
        })?,
    )?;
    globals.set(
        "Var",
        lua.create_function(|lua, args: MultiValue| {
            let name = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            if name == "GameCommand" {
                return Ok(Value::Table(create_game_command_table(lua)?));
            }
            Ok(Value::String(lua.create_string(&name)?))
        })?,
    )?;
    globals.set("ActorUtil", create_actor_util_table(lua, song_dir)?)?;
    let find_files_song_dir = song_dir.to_path_buf();
    globals.set(
        "findFiles",
        lua.create_function(move |lua, args: MultiValue| {
            find_compat_files(lua, &find_files_song_dir, &args)
        })?,
    )?;
    globals.set(
        "cleanGSub",
        lua.create_function(|lua, args: MultiValue| {
            let text = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let needle = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let replacement = args
                .get(2)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(
                lua.create_string(text.replace(&needle, &replacement))?,
            ))
        })?,
    )?;
    globals.set(
        "range",
        lua.create_function(|lua, args: MultiValue| create_range_table(lua, &args))?,
    )?;
    globals.set(
        "stringify",
        lua.create_function(|lua, args: MultiValue| stringify_lua_table(lua, &args))?,
    )?;
    globals.set(
        "map",
        lua.create_function(|lua, (function, table): (Function, Table)| {
            map_lua_table(lua, &function, &table)
        })?,
    )?;
    globals.set(
        "deduplicate",
        lua.create_function(|lua, table: Table| deduplicate_lua_table(lua, &table))?,
    )?;
    globals.set(
        "TableToString",
        lua.create_function(|_, args: MultiValue| Ok(lua_table_to_string(&args)))?,
    )?;
    globals.set(
        "force_to_range",
        lua.create_function(|_, (min, number, max): (f64, f64, f64)| Ok(number.clamp(min, max)))?,
    )?;
    globals.set(
        "wrapped_index",
        lua.create_function(|_, (start, offset, set_size): (i64, i64, i64)| {
            Ok(if set_size <= 0 {
                0
            } else {
                (start + offset - 1).rem_euclid(set_size) + 1
            })
        })?,
    )?;
    globals.set(
        "GetVersionParts",
        lua.create_function(|lua, args: MultiValue| {
            let version = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_else(|| SONG_LUA_PRODUCT_VERSION.to_string());
            create_version_parts_table(lua, &version)
        })?,
    )?;
    globals.set(
        "GetProductVersion",
        lua.create_function(|lua, _args: MultiValue| {
            create_version_parts_table(lua, SONG_LUA_PRODUCT_VERSION)
        })?,
    )?;
    globals.set(
        "IsProductVersion",
        lua.create_function(|_, args: MultiValue| Ok(is_product_version(&args)))?,
    )?;
    globals.set(
        "IsMinimumProductVersion",
        lua.create_function(|_, args: MultiValue| Ok(is_minimum_product_version(&args)))?,
    )?;
    globals.set(
        "IsITGmania",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    globals.set(
        "StepManiaVersionIsSupported",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    globals.set(
        "MinimumVersionString",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("1.2.0")?))
        })?,
    )?;
    globals.set(
        "CurrentGameIsSupported",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    globals.set(
        "GetThemeVersion",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(SONG_LUA_PRODUCT_VERSION)?))
        })?,
    )?;
    globals.set(
        "GetAuthor",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(
                lua.create_string("DeadSync song-Lua compat")?,
            ))
        })?,
    )?;
    globals.set(
        "SupportsRenderToTexture",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    globals.set(
        "BackgroundFilterValues",
        lua.create_function(|lua, _args: MultiValue| create_background_filter_values(lua))?,
    )?;
    globals.set(
        "NumJudgmentsAvailable",
        lua.create_function(|_, _args: MultiValue| Ok(5_i64))?,
    )?;
    globals.set(
        "DetermineTimingWindow",
        lua.create_function(|_, args: MultiValue| {
            let offset = args
                .front()
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0)
                .abs();
            for index in 1..=5 {
                if offset <= timing_window_seconds(index, "", false) {
                    return Ok(index);
                }
            }
            Ok(5)
        })?,
    )?;
    globals.set(
        "GetCredits",
        lua.create_function(|lua, _args: MultiValue| create_credits_table(lua))?,
    )?;
    globals.set(
        "GetStepsCredit",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    globals.set(
        "IsSpooky",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "StripSpriteHints",
        lua.create_function(|_, filename: String| Ok(strip_sprite_hints(&filename)))?,
    )?;
    globals.set(
        "GetJudgmentGraphics",
        lua.create_function(|lua, _args: MultiValue| create_string_array(lua, &["None"]))?,
    )?;
    globals.set(
        "GetHoldJudgments",
        lua.create_function(|lua, _args: MultiValue| create_string_array(lua, &["None"]))?,
    )?;
    globals.set(
        "GetHeldMissGraphics",
        lua.create_function(|lua, _args: MultiValue| create_string_array(lua, &["None"]))?,
    )?;
    globals.set(
        "GetComboFonts",
        lua.create_function(|lua, _args: MultiValue| create_string_array(lua, &["None"]))?,
    )?;
    globals.set(
        "GetColumnMapping",
        lua.create_function(|lua, _args: MultiValue| {
            create_index_array(lua, SONG_LUA_NOTE_COLUMNS)
        })?,
    )?;
    globals.set(
        "GetPlayerOptionsString",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    globals.set(
        "GetFallbackBanner",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(
                SONG_LUA_THEME_PATH_PREFIX.trim_end_matches('/'),
            )?))
        })?,
    )?;
    globals.set(
        "TotalCourseLength",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    globals.set(
        "TotalCourseLengthPlayed",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    globals.set(
        "IsGameAndMenuButton",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    for name in ["LoadGuest", "LoadProfileCustom", "SaveProfileCustom"] {
        globals.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(true)
            })?,
        )?;
    }
    for name in ["GetAvatarPath", "GetPlayerAvatarPath"] {
        globals.set(
            name,
            lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    globals.set(
        "getAuthorTable",
        lua.create_function(|lua, args: MultiValue| create_author_table(lua, args.front()))?,
    )?;
    globals.set(
        "courseLengthBySong",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    globals.set(
        "SecondsToHMMSS",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(
                lua.create_string(seconds_to_hhmmss(seconds))?,
            ))
        })?,
    )?;
    globals.set(
        "ParseChartInfo",
        lua.create_function(|lua, args: MultiValue| parse_chart_info(lua, &args))?,
    )?;
    globals.set(
        "ivalues",
        lua.create_function(|lua, table: Table| {
            let mut index = 0_i64;
            lua.create_function_mut(move |_, ()| {
                index += 1;
                table.raw_get::<Value>(index)
            })
        })?,
    )?;
    globals.set("Trace", lua.create_function(|_, _msg: String| Ok(()))?)?;
    globals.set("debug", create_debug_table(lua)?)?;
    globals.set("lua", create_lua_compat_table(lua, song_dir)?)?;
    globals.set(
        "Warn",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "color",
        lua.create_function(|lua, args: MultiValue| {
            Ok(match read_color_call(&args) {
                Some(color) => Value::Table(make_color_table(lua, color)?),
                None => Value::Nil,
            })
        })?,
    )?;
    globals.set(
        "lerp_color",
        lua.create_function(|lua, args: MultiValue| {
            let Some(percent) = args.front().cloned().and_then(read_f32) else {
                return Ok(Value::Nil);
            };
            let Some(a) = args.get(1).cloned().and_then(read_color_value) else {
                return Ok(Value::Nil);
            };
            let Some(b) = args.get(2).cloned().and_then(read_color_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(make_color_table(
                lua,
                [
                    a[0] + (b[0] - a[0]) * percent,
                    a[1] + (b[1] - a[1]) * percent,
                    a[2] + (b[2] - a[2]) * percent,
                    a[3] + (b[3] - a[3]) * percent,
                ],
            )?))
        })?,
    )?;
    globals.set("Color", create_color_constants_table(lua)?)?;
    install_theme_color_helpers(lua, &globals)?;
    for (name, value) in [
        ("left", "left"),
        ("center", "center"),
        ("middle", "middle"),
        ("right", "right"),
        ("top", "top"),
        ("bottom", "bottom"),
        ("HorizAlign_Left", "HorizAlign_Left"),
        ("HorizAlign_Center", "HorizAlign_Center"),
        ("HorizAlign_Right", "HorizAlign_Right"),
        ("VertAlign_Top", "VertAlign_Top"),
        ("VertAlign_Middle", "VertAlign_Middle"),
        ("VertAlign_Bottom", "VertAlign_Bottom"),
    ] {
        globals.set(name, value)?;
    }
    globals.set(
        "setfenv",
        lua.create_function(|lua, (target, env): (Value, Table)| match target {
            Value::Function(function) => {
                let _ = function.set_environment(env.clone())?;
                Ok(Value::Function(function))
            }
            Value::Integer(_) | Value::Number(_) => {
                if let Some(current_env) = lua
                    .globals()
                    .get::<Option<Table>>("__songlua_current_chunk_env")?
                {
                    current_env.set("__songlua_env_target", env)?;
                }
                Ok(target)
            }
            _ => Ok(target),
        })?,
    )?;
    globals.set(
        "loadstring",
        lua.create_function(|lua, (code, chunk_name): (String, Option<String>)| {
            let mut chunk = lua.load(&code);
            if let Some(chunk_name) = chunk_name.as_deref() {
                chunk = chunk.set_name(chunk_name);
            }
            Ok(Value::Function(chunk.into_function()?))
        })?,
    )?;
    let fileman = lua.create_table()?;
    let song_dir = song_dir.to_path_buf();
    let listing_song_dir = song_dir.clone();
    fileman.set(
        "GetDirListing",
        lua.create_function(move |lua, args: MultiValue| {
            fileman_dir_listing_table(lua, &listing_song_dir, &args).map(Value::Table)
        })?,
    )?;
    let file_song_dir = song_dir.clone();
    fileman.set(
        "DoesFileExist",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(resolve_compat_path(&file_song_dir, raw_path.as_str()).exists())
        })?,
    )?;
    let size_song_dir = song_dir.clone();
    fileman.set(
        "GetFileSizeBytes",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(0_i64);
            };
            let size = resolve_compat_path(&size_song_dir, raw_path.as_str())
                .metadata()
                .ok()
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                .unwrap_or(0);
            Ok(size)
        })?,
    )?;
    fileman.set(
        "GetHashForFile",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    fileman.set(
        "Copy",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    for (name, value) in [("CreateDir", true), ("Remove", true), ("Unzip", false)] {
        fileman.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(value)
            })?,
        )?;
    }
    fileman.set(
        "FlushDirCache",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set("FILEMAN", fileman)?;
    globals.set("CRYPTMAN", create_cryptman_table(lua)?)?;
    globals.set("NETWORK", create_network_table(lua)?)?;
    globals.set("IniFile", create_ini_file_table(lua)?)?;
    globals.set("RageFileUtil", create_rage_file_util_table(lua)?)?;
    globals.set(
        "JsonDecode",
        lua.create_function(|lua, value: Value| match value {
            Value::String(text) => {
                let bytes = text.as_bytes();
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .map(|value| json_to_lua_value(lua, value))
                    .transpose()?
                    .map_or_else(|| Ok(Value::Table(lua.create_table()?)), Ok)
            }
            _ => Ok(Value::Table(lua.create_table()?)),
        })?,
    )?;
    globals.set(
        "JsonEncode",
        lua.create_function(|_, args: MultiValue| {
            let value = args.front().cloned().unwrap_or(Value::Nil);
            Ok(serde_json::to_string(&lua_to_json_value(value, 0)).unwrap_or_default())
        })?,
    )?;
    globals.set(
        "BinaryToHex",
        lua.create_function(|lua, value: Value| {
            Ok(Value::String(
                lua.create_string(lua_binary_to_hex(value).as_str())?,
            ))
        })?,
    )?;
    globals.set(
        "CalculateExScore",
        lua.create_function(|_, _args: MultiValue| Ok((0.0_f32, 0_i64, 0_i64)))?,
    )?;
    globals.set(
        "GetExJudgmentCounts",
        lua.create_function(|lua, _args: MultiValue| create_ex_judgment_counts(lua))?,
    )?;
    globals.set(
        "GetTimingWindow",
        lua.create_function(|_, args: MultiValue| {
            let index = args
                .front()
                .cloned()
                .and_then(timing_window_arg_index)
                .unwrap_or(1);
            let mode = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let tenms = args.get(2).is_some_and(truthy);
            Ok(timing_window_seconds(index, &mode, tenms))
        })?,
    )?;
    globals.set(
        "GetWorstJudgment",
        lua.create_function(|_, value: Value| Ok(worst_judgment_from_offsets(value)))?,
    )?;
    Ok(())
}

fn fileman_dir_listing_table(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let Some(raw_path) = method_arg(args, 0).cloned().and_then(read_string) else {
        return Ok(table);
    };
    let only_dirs = method_arg(args, 1)
        .cloned()
        .and_then(read_boolish)
        .unwrap_or(false);
    let return_path_too = method_arg(args, 2)
        .cloned()
        .and_then(read_boolish)
        .unwrap_or(false);

    let path = resolve_compat_path(song_dir, raw_path.as_str());
    let mut entries = Vec::new();
    if path.is_dir() {
        entries = fileman_read_dir(&path, None, only_dirs, return_path_too)?;
    } else if let Some(pattern) = path.file_name().and_then(|name| name.to_str())
        && pattern.bytes().any(|byte| matches!(byte, b'*' | b'?'))
        && let Some(parent) = path.parent()
    {
        entries = fileman_read_dir(parent, Some(pattern), only_dirs, return_path_too)?;
    } else if path.exists() && (!only_dirs || path.is_dir()) {
        entries.push(fileman_entry_name(&path, return_path_too));
    }

    entries.sort_unstable();
    for (idx, entry) in entries.into_iter().enumerate() {
        table.raw_set(idx + 1, entry)?;
    }
    Ok(table)
}

fn fileman_read_dir(
    path: &Path,
    pattern: Option<&str>,
    only_dirs: bool,
    return_path_too: bool,
) -> mlua::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)
        .map_err(mlua::Error::external)?
        .filter_map(Result::ok)
    {
        let entry_path = entry.path();
        if only_dirs && !entry_path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        if pattern.is_some_and(|pattern| !wildcard_matches(pattern, &name)) {
            continue;
        }
        entries.push(if return_path_too {
            file_path_string(entry_path.as_path())
        } else {
            name
        });
    }
    Ok(entries)
}

fn fileman_entry_name(path: &Path, return_path_too: bool) -> String {
    if return_path_too {
        return file_path_string(path);
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let (mut pattern_idx, mut text_idx) = (0, 0);
    let (mut star_idx, mut star_text_idx) = (None, 0);
    while text_idx < text.len() {
        if pattern_idx < pattern.len()
            && (pattern[pattern_idx] == b'?' || pattern[pattern_idx] == text[text_idx])
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if pattern_idx < pattern.len() && pattern[pattern_idx] == b'*' {
            star_idx = Some(pattern_idx);
            star_text_idx = text_idx;
            pattern_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            star_text_idx += 1;
            text_idx = star_text_idx;
        } else {
            return false;
        }
    }
    pattern[pattern_idx..].iter().all(|byte| *byte == b'*')
}

fn create_cryptman_table(lua: &Lua) -> mlua::Result<Table> {
    let cryptman = lua.create_table()?;
    for name in ["SHA1File", "SHA1String"] {
        cryptman.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(&[0_u8; 20])?))
            })?,
        )?;
    }
    for name in ["SHA256File", "SHA256String"] {
        cryptman.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(&[0_u8; 32])?))
            })?,
        )?;
    }
    cryptman.set(
        "GenerateRandomUUID",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(
                lua.create_string("00000000-0000-4000-8000-000000000000")?,
            ))
        })?,
    )?;
    cryptman.set(
        "SignFileToFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    Ok(cryptman)
}

fn create_network_table(lua: &Lua) -> mlua::Result<Table> {
    let network = lua.create_table()?;
    network.set(
        "IsUrlAllowed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    network.set(
        "HttpRequest",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            create_network_response_table(lua)
        })?,
    )?;
    network.set(
        "WebSocket",
        lua.create_function(|lua, _args: MultiValue| create_websocket_table(lua))?,
    )?;
    network.set(
        "EncodeQueryParameters",
        lua.create_function(|_, args: MultiValue| {
            let Some(Value::Table(query)) = method_arg(&args, 0).cloned() else {
                return Ok(String::new());
            };
            encode_query_params(query)
        })?,
    )?;
    Ok(network)
}

fn create_split_table(lua: &Lua, text: &str, separator: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if separator.is_empty() {
        if text.is_empty() {
            table.raw_set(1, "")?;
        } else {
            for (idx, value) in text.chars().enumerate() {
                table.raw_set(idx + 1, value.to_string())?;
            }
        }
        return Ok(table);
    }
    for (idx, value) in text.split(separator).enumerate() {
        table.raw_set(idx + 1, value)?;
    }
    Ok(table)
}

fn path_basename(text: &str) -> &str {
    let trimmed = text.trim_end_matches(|c| c == '/' || c == '\\');
    trimmed
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or_default()
}

fn create_range_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
    let Some(mut start) = args.front().cloned().and_then(read_f32) else {
        return Ok(Value::Nil);
    };
    let stop = if let Some(stop) = args.get(1).cloned().and_then(read_f32) {
        stop
    } else {
        let stop = start;
        start = 1.0;
        stop
    };
    let mut step = args.get(2).cloned().and_then(read_f32).unwrap_or(1.0);
    if step.abs() <= f32::EPSILON {
        return Ok(Value::Table(lua.create_table()?));
    }
    if step > 0.0 && start > stop {
        step = -step;
    }
    if step < 0.0 && start < stop {
        return Ok(Value::Table(lua.create_table()?));
    }

    let table = lua.create_table()?;
    let mut index = 1;
    let mut value = start;
    while (step > 0.0 && value <= stop + f32::EPSILON)
        || (step < 0.0 && value >= stop - f32::EPSILON)
    {
        table.raw_set(index, lua_number_value(value))?;
        index += 1;
        value += step;
        if index > 10_000 {
            break;
        }
    }
    Ok(Value::Table(table))
}

fn lua_number_value(value: f32) -> Value {
    if value.is_finite() && value.fract().abs() <= f32::EPSILON {
        Value::Integer(value as i64)
    } else {
        Value::Number(value.into())
    }
}

fn stringify_lua_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
    let Some(Value::Table(table)) = args.front().cloned() else {
        return Ok(Value::Nil);
    };
    let form = args.get(1).cloned().and_then(read_string);
    let format = lua
        .globals()
        .get::<Table>("string")?
        .get::<Function>("format")?;
    let out = lua.create_table()?;
    for (idx, value) in table.sequence_values::<Value>().enumerate() {
        let value = value?;
        let text = if let Some(form) = form.as_deref()
            && matches!(value, Value::Integer(_) | Value::Number(_))
        {
            let mut call_args = MultiValue::new();
            call_args.push_back(Value::String(lua.create_string(form)?));
            call_args.push_back(value);
            lua_text_value(format.call::<Value>(call_args)?)?
        } else {
            lua_text_value(value)?
        };
        out.raw_set(idx + 1, text)?;
    }
    Ok(Value::Table(out))
}

fn map_lua_table(lua: &Lua, function: &Function, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    for (idx, value) in table.sequence_values::<Value>().enumerate() {
        out.raw_set(idx + 1, function.call::<Value>(value?)?)?;
    }
    Ok(out)
}

fn deduplicate_lua_table(lua: &Lua, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let mut seen = Vec::new();
    let mut out_index = 1;
    for value in table.sequence_values::<Value>() {
        let value = value?;
        if seen.iter().any(|seen| lua_values_equal(seen, &value)) {
            continue;
        }
        seen.push(value.clone());
        out.raw_set(out_index, value)?;
        out_index += 1;
    }
    Ok(out)
}

fn rotate_lua_table(lua: &Lua, args: &MultiValue, left: bool) -> mlua::Result<Table> {
    let Some(Value::Table(input)) = args.front() else {
        return lua.create_table();
    };
    let len = input.raw_len();
    if len == 0 {
        return lua.create_table();
    }
    let shift = args.get(1).cloned().and_then(read_i32_value).unwrap_or(1) as i64;
    let len_i64 = len as i64;
    let out = lua.create_table()?;
    for index in 1..=len_i64 {
        let source = if left {
            (index + shift - 1).rem_euclid(len_i64) + 1
        } else {
            (index - shift - 1).rem_euclid(len_i64) + 1
        };
        out.raw_set(index, input.raw_get::<Value>(source)?)?;
    }
    Ok(out)
}

fn find_compat_files(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<Table> {
    let dir = args
        .front()
        .cloned()
        .and_then(read_string)
        .unwrap_or_default();
    let extension = args
        .get(1)
        .cloned()
        .and_then(read_string)
        .unwrap_or_else(|| "ogg".to_string())
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let path = resolve_compat_path(song_dir, &dir);
    let mut files = if path.is_dir() {
        fs::read_dir(path)
            .map_err(mlua::Error::external)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case(&extension))
            })
            .map(|path| file_path_string(path.as_path()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    files.sort_unstable();
    let table = lua.create_table()?;
    for (index, file) in files.into_iter().enumerate() {
        table.raw_set(index + 1, file)?;
    }
    Ok(table)
}

fn create_actor_util_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let resolve_song_dir = song_dir.to_path_buf();
    table.set(
        "ResolvePath",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(path) = args.front().cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let resolved = resolve_compat_path(&resolve_song_dir, &path);
            let out = if resolved.exists() {
                file_path_string(resolved.as_path())
            } else {
                path
            };
            Ok(Value::String(lua.create_string(&out)?))
        })?,
    )?;
    table.set(
        "GetFileType",
        lua.create_function(|lua, args: MultiValue| {
            let file_type = args
                .front()
                .cloned()
                .and_then(read_string)
                .map(|path| actor_util_file_type(&path))
                .unwrap_or("FileType_Unknown");
            Ok(Value::String(lua.create_string(file_type)?))
        })?,
    )?;
    table.set(
        "IsRegisteredClass",
        lua.create_function(|_, args: MultiValue| {
            let registered = args
                .front()
                .cloned()
                .and_then(read_string)
                .is_some_and(|name| actor_util_class_registered(&name));
            Ok(registered)
        })?,
    )?;
    for method in ["LoadAllCommands", "LoadAllCommandsFromName"] {
        table.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    table.set(
        "LoadAllCommandsAndSetXY",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
}

fn actor_util_class_registered(name: &str) -> bool {
    matches!(
        name,
        "Actor"
            | "ActorFrame"
            | "Sprite"
            | "Banner"
            | "ActorMultiVertex"
            | "Sound"
            | "BitmapText"
            | "RollingNumbers"
            | "GraphDisplay"
            | "SongMeterDisplay"
            | "CourseContentsList"
            | "DeviceList"
            | "InputList"
            | "Model"
            | "Quad"
            | "ActorProxy"
            | "ActorFrameTexture"
    )
}

fn actor_util_file_type(path: &str) -> &'static str {
    let path = Path::new(path);
    if path.is_dir() {
        return "FileType_Directory";
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp") => "FileType_Bitmap",
        Some("mp4" | "avi" | "mov" | "mkv" | "webm" | "mpeg" | "mpg") => "FileType_Movie",
        Some("ogg" | "oga" | "mp3" | "wav" | "flac" | "opus") => "FileType_Sound",
        Some("lua") => "FileType_Lua",
        Some("xml" | "ini" | "txt" | "json" | "ssc" | "sm") => "FileType_Text",
        _ => "FileType_Unknown",
    }
}

fn create_game_command_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    for name in [
        "GetText",
        "GetName",
        "GetScreen",
        "GetIcon",
        "GetProfileID",
        "GetAnnouncer",
        "GetPreferredModifiers",
        "GetStageModifiers",
    ] {
        set_string_method(lua, &table, name, "")?;
    }
    table.set(
        "GetMultiPlayer",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    table.set(
        "GetStyle",
        lua.create_function(|lua, _args: MultiValue| {
            current_gamestate_value(lua, "GetCurrentStyle")
        })?,
    )?;
    table.set(
        "GetSong",
        lua.create_function(|lua, _args: MultiValue| current_song_value(lua))?,
    )?;
    table.set(
        "GetSteps",
        lua.create_function(|lua, _args: MultiValue| current_steps_value(lua, 0))?,
    )?;
    table.set(
        "GetCourse",
        lua.create_function(|lua, _args: MultiValue| {
            current_gamestate_value(lua, "GetCurrentCourse")
        })?,
    )?;
    table.set(
        "GetTrail",
        lua.create_function(|lua, _args: MultiValue| {
            current_gamestate_player_value(lua, "GetCurrentTrail", 0)
        })?,
    )?;
    table.set(
        "GetCharacter",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    table.set(
        "GetSongGroup",
        lua.create_function(|lua, _args: MultiValue| {
            let Value::Table(song) = current_song_value(lua)? else {
                return Ok(String::new());
            };
            let Some(method) = song.get::<Option<Function>>("GetGroupName")? else {
                return Ok(String::new());
            };
            method.call::<String>(song)
        })?,
    )?;
    table.set(
        "GetUrl",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    for (method, value) in [
        ("GetDifficulty", "Difficulty_Invalid"),
        ("GetCourseDifficulty", "Difficulty_Invalid"),
        ("GetPlayMode", "PlayMode_Invalid"),
        ("GetSortOrder", "SortOrder_Invalid"),
    ] {
        set_string_method(lua, &table, method, value)?;
    }
    table.set(
        "ApplyToStyle",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
}

fn lua_table_to_string(args: &MultiValue) -> String {
    let name = args
        .get(1)
        .cloned()
        .and_then(read_string)
        .unwrap_or_else(|| "Value".to_string());
    match args.front() {
        Some(Value::Table(table)) if table.raw_len() == 0 => format!("{name} = {{}}"),
        Some(Value::Table(table)) => format!("{name} = {{...{} item(s)}}", table.raw_len()),
        Some(value) => format!(
            "{name} = {}",
            lua_text_value(value.clone()).unwrap_or_default()
        ),
        None => format!("{name} = nil"),
    }
}

fn create_version_parts_table(lua: &Lua, version: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, part) in version_parts(version).into_iter().enumerate() {
        table.raw_set(index + 1, part)?;
    }
    Ok(table)
}

fn version_parts(version: &str) -> [i64; 3] {
    let mut parts = [0_i64; 3];
    for (index, part) in version.trim().split('.').take(3).enumerate() {
        let digits = part
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .collect::<Vec<_>>();
        parts[index] = std::str::from_utf8(&digits)
            .ok()
            .and_then(|digits| digits.parse::<i64>().ok())
            .unwrap_or(0);
    }
    parts
}

fn version_args(args: &MultiValue) -> Vec<i64> {
    if let Some(Value::String(version)) = args.front() {
        if let Ok(version) = version.to_str() {
            return version_parts(version.as_ref()).into_iter().collect();
        }
    }
    args.iter()
        .filter_map(|value| read_f32(value.clone()))
        .map(|value| value.round() as i64)
        .collect()
}

fn is_product_version(args: &MultiValue) -> bool {
    let expected = version_args(args);
    if expected.is_empty() {
        return false;
    }
    let product = version_parts(SONG_LUA_PRODUCT_VERSION);
    expected
        .into_iter()
        .enumerate()
        .all(|(index, value)| product.get(index).is_some_and(|part| *part == value))
}

fn is_minimum_product_version(args: &MultiValue) -> bool {
    let expected = version_args(args);
    if expected.is_empty() {
        return true;
    }
    let product = version_parts(SONG_LUA_PRODUCT_VERSION);
    for (index, expected) in expected.into_iter().enumerate() {
        let product = product.get(index).copied().unwrap_or(0);
        if product != expected {
            return product > expected;
        }
    }
    true
}

fn create_author_table(lua: &Lua, steps: Option<&Value>) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let Some(Value::Table(steps)) = steps else {
        return Ok(out);
    };
    let mut values = Vec::new();
    for method in ["GetDescription", "GetAuthorCredit", "GetChartName"] {
        if let Some(text) = call_string_method(steps, method)? {
            if !text.is_empty() && !values.iter().any(|value| value == &text) {
                values.push(text);
            }
        }
    }
    for (index, value) in values.into_iter().enumerate() {
        out.raw_set(index + 1, value)?;
    }
    Ok(out)
}

fn call_string_method(table: &Table, name: &str) -> mlua::Result<Option<String>> {
    let Some(function) = table.get::<Option<Function>>(name)? else {
        return Ok(None);
    };
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    Ok(Some(lua_text_value(function.call::<Value>(args)?)?))
}

fn parse_chart_info(lua: &Lua, args: &MultiValue) -> mlua::Result<Table> {
    let player = args
        .get(1)
        .and_then(player_index_from_value)
        .map(player_short_name)
        .unwrap_or("P1");
    let globals = lua.globals();
    let sl = match globals.get::<Option<Table>>("SL")? {
        Some(table) => table,
        None => {
            let table = lua.create_table()?;
            globals.set("SL", table.clone())?;
            table
        }
    };
    let player_table = match sl.get::<Option<Table>>(player)? {
        Some(table) => table,
        None => {
            let table = lua.create_table()?;
            sl.set(player, table.clone())?;
            table
        }
    };
    let streams = match player_table.get::<Option<Table>>("Streams")? {
        Some(table) => table,
        None => {
            let table = create_sl_streams(lua)?;
            player_table.set("Streams", table.clone())?;
            table
        }
    };
    init_sl_streams(lua, &streams)?;
    note_song_lua_side_effect(lua)?;
    Ok(streams)
}

fn create_background_filter_values(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Off", 0)?;
    table.set("Dark", 50)?;
    table.set("Darker", 75)?;
    table.set("Darkest", 95)?;
    table.raw_set(0, 0)?;
    table.raw_set(50, 50)?;
    table.raw_set(75, 75)?;
    table.raw_set(95, 95)?;
    Ok(table)
}

fn create_gameplay_layout(lua: &Lua, screen_center_y: f32, reverse: bool) -> mlua::Result<Table> {
    let combo_y = screen_center_y + if reverse { -30.0 } else { 30.0 };
    let judgment_y = screen_center_y + if reverse { 30.0 } else { -30.0 };
    let sign = if reverse { -1.0 } else { 1.0 };
    let table = lua.create_table()?;
    table.set("Combo", create_layout_slot(lua, combo_y, None)?)?;
    table.set("ErrorBar", create_layout_slot(lua, judgment_y, Some(30.0))?)?;
    table.set(
        "MeasureCounter",
        create_layout_slot(lua, judgment_y + sign * 28.0, None)?,
    )?;
    table.set(
        "SubtractiveScoring",
        create_layout_slot(lua, judgment_y - sign * 28.0, None)?,
    )?;
    Ok(table)
}

fn create_layout_slot(lua: &Lua, y: f32, max_height: Option<f32>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("y", y)?;
    if let Some(max_height) = max_height {
        table.set("maxHeight", max_height)?;
    }
    Ok(table)
}

fn create_credits_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Credits", 0)?;
    table.set("Remainder", 0)?;
    table.set("CoinsPerCredit", 1)?;
    Ok(table)
}

fn create_index_array(lua: &Lua, len: usize) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for index in 1..=len {
        table.raw_set(index, index)?;
    }
    Ok(table)
}

fn strip_sprite_hints(filename: &str) -> String {
    let mut text = filename.replace(" (doubleres)", "");
    if text
        .as_bytes()
        .get(text.len().saturating_sub(4)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".png"))
    {
        text.truncate(text.len() - 4);
    }
    if let Some(space) = text.rfind(' ')
        && frame_hint(&text[space + 1..])
    {
        text.truncate(space);
    }
    text
}

fn frame_hint(text: &str) -> bool {
    let Some((wide, tall)) = text.split_once('x') else {
        return false;
    };
    !wide.is_empty()
        && !tall.is_empty()
        && wide.bytes().all(|byte| byte.is_ascii_digit())
        && tall.bytes().all(|byte| byte.is_ascii_digit())
}

fn create_network_response_table(lua: &Lua) -> mlua::Result<Table> {
    let response = lua.create_table()?;
    response.set("status", 0)?;
    response.set("code", 0)?;
    response.set("body", "")?;
    response.set("error", "offline")?;
    response.set("headers", lua.create_table()?)?;
    response.set(
        "IsFinished",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    let response_for_get = response.clone();
    response.set(
        "GetResponse",
        lua.create_function(move |_, _args: MultiValue| Ok(response_for_get.clone()))?,
    )?;
    response.set(
        "Cancel",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(response)
}

fn create_websocket_table(lua: &Lua) -> mlua::Result<Table> {
    let websocket = lua.create_table()?;
    websocket.set("is_open", false)?;
    websocket.set(
        "IsOpen",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    for name in ["Send", "Close"] {
        websocket.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(websocket)
}

fn create_ini_file_table(lua: &Lua) -> mlua::Result<Table> {
    let ini = lua.create_table()?;
    ini.set(
        "ReadFile",
        lua.create_function(|lua, args: MultiValue| {
            let path = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let table = lua.create_table()?;
            if path.ends_with("ThemeInfo.ini") {
                let info = lua.create_table()?;
                info.set("DisplayName", SONG_LUA_THEME_NAME)?;
                info.set("Version", SONG_LUA_PRODUCT_VERSION)?;
                info.set("Author", "DeadSync song-Lua compat")?;
                table.set("ThemeInfo", info)?;
            }
            Ok(table)
        })?,
    )?;
    ini.set(
        "WriteFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    Ok(ini)
}

fn create_rage_file_util_table(lua: &Lua) -> mlua::Result<Table> {
    let util = lua.create_table()?;
    util.set(
        "CreateRageFile",
        lua.create_function(|lua, _args: MultiValue| create_rage_file_table(lua))?,
    )?;
    Ok(util)
}

fn create_rage_file_table(lua: &Lua) -> mlua::Result<Table> {
    let file = lua.create_table()?;
    file.set(
        "Open",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    file.set(
        "Write",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    file.set(
        "Read",
        lua.create_function(|_, _args: MultiValue| Ok(String::new()))?,
    )?;
    for name in ["Close", "destroy"] {
        file.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(file)
}

fn create_debug_table(lua: &Lua) -> mlua::Result<Table> {
    let debug = lua.create_table()?;
    debug.set(
        "getinfo",
        lua.create_function(|lua, args: MultiValue| {
            let globals = lua.globals();
            let source = globals
                .get::<Option<String>>("__songlua_current_script_path")?
                .map(|path| format!("@{path}"))
                .unwrap_or_else(|| "=[songlua]".to_string());
            let info = lua.create_table()?;
            info.set("source", source)?;
            info.set("short_src", info.get::<String>("source")?)?;
            info.set("what", "Lua")?;
            info.set("currentline", 0)?;
            info.set("linedefined", 0)?;
            info.set("lastlinedefined", 0)?;
            if args.front().is_some() {
                info.set("namewhat", "")?;
            }
            Ok(info)
        })?,
    )?;
    debug.set(
        "getupvalue",
        lua.create_function(|lua, args: MultiValue| {
            let Some(function) = args.front().cloned().and_then(|value| match value {
                Value::Function(function) => Some(function),
                _ => None,
            }) else {
                return Ok((Value::Nil, Value::Nil));
            };
            let Some(index) = args.get(1).cloned().and_then(|value| match value {
                Value::Integer(value) => Some(value),
                Value::Number(value) => Some(value as i64),
                _ => None,
            }) else {
                return Ok((Value::Nil, Value::Nil));
            };
            // SAFETY: exec_raw owns the temporary stack frame for this call. We only
            // read the pushed function/index arguments, call Lua's debug API to fetch
            // a single upvalue, then replace the frame contents with plain Lua return
            // values before exec_raw converts them back into mlua Values.
            unsafe {
                lua.exec_raw((function, index), |state| {
                    let upvalue_index = ffi::lua_tointeger(state, 2) as c_int;
                    let name = ffi::lua_getupvalue(state, 1, upvalue_index);
                    ffi::lua_remove(state, 2);
                    ffi::lua_remove(state, 1);
                    if name.is_null() {
                        ffi::lua_pushnil(state);
                        ffi::lua_pushnil(state);
                        return;
                    }
                    ffi::lua_pushstring(state, name);
                    ffi::lua_insert(state, -2);
                })
            }
        })?,
    )?;
    debug.set(
        "traceback",
        lua.create_function(|_, args: MultiValue| {
            let message = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(message)
        })?,
    )?;
    Ok(debug)
}

fn resolve_compat_path(song_dir: &Path, raw_path: &str) -> PathBuf {
    let path = Path::new(raw_path.trim());
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        song_dir.join(path)
    }
}

fn create_lua_compat_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let read_song_dir = song_dir.to_path_buf();
    table.set(
        "ReadFile",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let path = resolve_compat_path(&read_song_dir, &raw_path);
            match fs::read_to_string(path) {
                Ok(text) => Ok(Value::String(lua.create_string(&text)?)),
                Err(_) => Ok(Value::Nil),
            }
        })?,
    )?;
    table.set(
        "WriteFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    table.set(
        "ReportScriptError",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
}

fn json_to_lua_value(lua: &Lua, value: serde_json::Value) -> mlua::Result<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(value) => Value::Boolean(value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(Value::Integer)
            .or_else(|| value.as_f64().map(Value::Number))
            .unwrap_or(Value::Nil),
        serde_json::Value::String(value) => Value::String(lua.create_string(value)?),
        serde_json::Value::Array(values) => {
            let table = lua.create_table()?;
            for (index, value) in values.into_iter().enumerate() {
                table.raw_set(index + 1, json_to_lua_value(lua, value)?)?;
            }
            Value::Table(table)
        }
        serde_json::Value::Object(values) => {
            let table = lua.create_table()?;
            for (key, value) in values {
                table.set(key, json_to_lua_value(lua, value)?)?;
            }
            Value::Table(table)
        }
    })
}

fn lua_to_json_value(value: Value, depth: usize) -> serde_json::Value {
    if depth >= 16 {
        return serde_json::Value::Null;
    }
    match value {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(value) => serde_json::Value::Bool(value),
        Value::Integer(value) => serde_json::Value::Number(value.into()),
        Value::Number(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(value) => serde_json::Value::String(value.to_string_lossy()),
        Value::Table(table) => lua_table_to_json_value(table, depth + 1),
        _ => serde_json::Value::Null,
    }
}

fn lua_table_to_json_value(table: Table, depth: usize) -> serde_json::Value {
    let len = table.raw_len();
    let mut array = vec![serde_json::Value::Null; len];
    let mut object = serde_json::Map::new();
    let mut is_array = len > 0;
    for pair in table.pairs::<Value, Value>() {
        let Ok((key, value)) = pair else {
            continue;
        };
        let json_value = lua_to_json_value(value, depth);
        match key {
            Value::Integer(index) if is_array && index >= 1 && index as usize <= len => {
                array[index as usize - 1] = json_value;
            }
            Value::String(key) => {
                is_array = false;
                object.insert(key.to_string_lossy(), json_value);
            }
            Value::Integer(index) => {
                is_array = false;
                object.insert(index.to_string(), json_value);
            }
            Value::Number(index) => {
                is_array = false;
                object.insert(index.to_string(), json_value);
            }
            Value::Boolean(key) => {
                is_array = false;
                object.insert(key.to_string(), json_value);
            }
            _ => {}
        }
    }
    if is_array {
        serde_json::Value::Array(array)
    } else {
        serde_json::Value::Object(object)
    }
}

fn lua_binary_to_hex(value: Value) -> String {
    let Value::String(text) = value else {
        return String::new();
    };
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter().copied() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn encode_query_params(query: Table) -> mlua::Result<String> {
    let mut parts = Vec::new();
    for pair in query.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Some(key) = query_value_text(key) else {
            continue;
        };
        let value = query_value_text(value).unwrap_or_default();
        parts.push(format!(
            "{}={}",
            url_encode_component(&key),
            url_encode_component(&value)
        ));
    }
    parts.sort_unstable();
    Ok(parts.join("&"))
}

fn query_value_text(value: Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.to_string_lossy()),
        Value::Integer(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

fn url_encode_component(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for byte in text.as_bytes().iter().copied() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

fn timing_window_arg_index(value: Value) -> Option<i32> {
    read_i32_value(value.clone()).or_else(|| {
        let text = read_string(value)?;
        let (_, suffix) = text.rsplit_once('W')?;
        suffix.parse::<i32>().ok()
    })
}

fn timing_window_seconds(index: i32, mode: &str, tenms: bool) -> f32 {
    if mode.eq_ignore_ascii_case("FA+") && tenms && index == 1 {
        return 0.0085 + 0.0015;
    }
    let windows = if mode.eq_ignore_ascii_case("FA+") {
        [0.0135, 0.0215, 0.043, 0.135, 0.18]
    } else {
        [0.0215, 0.043, 0.102, 0.135, 0.18]
    };
    let index = index.clamp(1, windows.len() as i32) as usize - 1;
    windows[index] + 0.0015
}

fn worst_judgment_from_offsets(value: Value) -> i32 {
    let Value::Table(offsets) = value else {
        return 1;
    };
    let mut worst = 1;
    for pair in offsets.sequence_values::<Value>() {
        let Ok(value) = pair else {
            continue;
        };
        for offset in timing_offsets_from_value(value) {
            let abs = offset.abs();
            let judgment = (1..=5)
                .find(|window| abs <= timing_window_seconds(*window, "", false))
                .unwrap_or(5);
            worst = worst.max(judgment);
        }
    }
    worst
}

fn timing_offsets_from_value(value: Value) -> Vec<f32> {
    if let Some(offset) = read_f32(value.clone()) {
        return vec![offset];
    }
    let Value::Table(table) = value else {
        return Vec::new();
    };
    let mut offsets = Vec::new();
    if let Ok(value) = table.raw_get::<Value>(2) {
        if let Some(offset) = read_f32(value) {
            offsets.push(offset);
        }
    }
    if matches!(table.raw_get::<Value>(6), Ok(value) if truthy(&value)) {
        if let Ok(value) = table.raw_get::<Value>(7) {
            if let Some(offset) = read_f32(value) {
                offsets.push(offset);
            }
        }
    }
    offsets
}

fn create_chunk_env_proxy(lua: &Lua, target: Table) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    proxy.set("__songlua_env_target", target.clone())?;
    let globals = lua.globals();
    let mt = lua.create_table()?;
    let proxy_for_index = proxy.clone();
    let globals_for_index = globals.clone();
    mt.set(
        "__index",
        lua.create_function(move |_, (_self, key): (Table, Value)| {
            let target: Table = proxy_for_index.get("__songlua_env_target")?;
            let value = target.get::<Value>(key.clone())?;
            if !matches!(value, Value::Nil) {
                return Ok(value);
            }
            globals_for_index.get::<Value>(key)
        })?,
    )?;
    let proxy_for_newindex = proxy.clone();
    let globals_for_newindex = globals.clone();
    mt.set(
        "__newindex",
        lua.create_function(move |_, (_self, key, value): (Table, Value, Value)| {
            let target: Table = proxy_for_newindex.get("__songlua_env_target")?;
            target.set(key.clone(), value.clone())?;
            if let Some(name) = read_string(key.clone())
                && is_compile_global_name(name.as_str())
            {
                globals_for_newindex.set(name, value)?;
            }
            Ok(())
        })?,
    )?;
    let _ = proxy.set_metatable(Some(mt));
    Ok(proxy)
}

fn initial_chunk_environment(lua: &Lua, path: &Path) -> mlua::Result<Table> {
    if path.file_name().and_then(|name| name.to_str()) == Some("std.lua")
        && path
            .parent()
            .and_then(|dir| dir.file_name())
            .and_then(|name| name.to_str())
            == Some("template")
        && let Some(xero) = lua.globals().get::<Option<Table>>("xero")?
    {
        return Ok(xero);
    }
    Ok(lua.globals())
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

fn create_sl_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Global", create_sl_global_table(lua, context)?)?;
    table.set("Colors", create_string_array(lua, SL_COLORS)?)?;
    table.set(
        "DecorativeColors",
        create_string_array(lua, SL_DECORATIVE_COLORS)?,
    )?;
    table.set("ITGDiffColors", create_string_array(lua, ITG_DIFF_COLORS)?)?;
    table.set("DDRDiffColors", create_string_array(lua, DDR_DIFF_COLORS)?)?;
    table.set("JudgmentColors", create_sl_judgment_colors(lua)?)?;
    table.set(
        "Preferences",
        create_sl_mode_table(lua, create_sl_preferences)?,
    )?;
    table.set("Metrics", create_sl_mode_table(lua, create_sl_metrics)?)?;
    table.set("GrooveStats", create_sl_groovestats(lua)?)?;
    table.set("ArrowCloud", create_sl_arrowcloud(lua)?)?;
    table.set("Downloads", lua.create_table()?)?;
    table.set("SRPG9", create_sl_srpg9(lua)?)?;
    for player in 0..LUA_PLAYERS {
        table.set(
            player_short_name(player),
            create_sl_player_table(lua, &context.players[player])?,
        )?;
    }
    Ok(table)
}

fn create_sl_global_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("GameMode", "ITG")?;
    table.set("ActiveColorIndex", SONG_LUA_ACTIVE_COLOR_INDEX)?;
    table.set(
        "ActiveModifiers",
        create_sl_active_mods(lua, context.song_music_rate)?,
    )?;
    table.set("Stages", create_sl_stages(lua)?)?;
    table.set("MenuTimer", create_sl_menu_timer(lua)?)?;
    table.set("ScreenAfter", create_sl_screen_after(lua)?)?;
    table.set("PrevScreenOptionsServiceRow", lua.create_table()?)?;
    table.set("Online", lua.create_table()?)?;
    table.set("SampleMusicLoops", true)?;
    table.set("SampleMusicStartsImmediately", false)?;
    table.set("GameplayReloadCheck", false)?;
    table.set("WheelLocked", false)?;
    table.set("ContinuesRemaining", 0)?;
    table.set("ColumnCueMinTime", 0.0)?;
    table.set("TimeAtSessionStart", 0.0)?;
    Ok(table)
}

fn create_sl_active_mods(lua: &Lua, music_rate: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("MusicRate", song_music_rate_value(music_rate))?;
    Ok(table)
}

fn create_sl_player_table(lua: &Lua, player: &SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("ApiKey", "")?;
    table.set("ArrowCloudApiKey", "")?;
    table.set("ActiveModifiers", create_sl_player_mods(lua, player)?)?;
    table.set("Stages", create_sl_stages(lua)?)?;
    table.set("Streams", create_sl_streams(lua)?)?;
    table.set("HighScores", create_sl_high_scores(lua)?)?;
    table.set("Favorites", lua.create_table()?)?;
    Ok(table)
}

fn create_sl_player_mods(lua: &Lua, player: &SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let (speed_type, speed_value) = speedmod_parts(player.speedmod);
    table.set("DataVisualizations", "None")?;
    table.set("ShowFaPlusWindow", false)?;
    table.set("ShowExScore", false)?;
    table.set("ShowHardEXScore", false)?;
    table.set("ShowFaPlusPane", false)?;
    table.set(
        "TimingWindows",
        create_bool_array(lua, &[true, true, true, false, false])?,
    )?;
    table.set("SpeedModType", speed_type)?;
    table.set("SpeedMod", speed_value)?;
    table.set("Mini", "0%")?;
    table.set("Spacing", "0%")?;
    table.set("VisualDelay", "0ms")?;
    table.set("BackgroundFilter", 0)?;
    table.set("HideTargets", false)?;
    table.set("HideSongBG", false)?;
    table.set("HideCombo", false)?;
    table.set("HideLifebar", false)?;
    table.set("HideScore", false)?;
    table.set("HideDanger", false)?;
    table.set("HideComboExplosions", false)?;
    table.set("ColumnFlashOnMiss", false)?;
    table.set("SubtractiveScoring", false)?;
    table.set("MeasureCounter", "None")?;
    table.set("MeasureCounterLeft", false)?;
    table.set("MeasureCounterUp", true)?;
    table.set("HideLookahead", false)?;
    table.set("MeasureLines", "Off")?;
    table.set("TargetScore", "Personal best")?;
    table.set("TargetScoreNumber", 100)?;
    table.set("ActionOnMissedTarget", "Nothing")?;
    table.set("Pacemaker", false)?;
    table.set("LifeMeterType", "Standard")?;
    table.set("NPSGraphAtTop", false)?;
    table.set("JudgmentTilt", false)?;
    table.set("TiltMultiplier", 1)?;
    table.set("ColumnCues", false)?;
    table.set("ColumnCountdown", false)?;
    table.set("ShowHeldMiss", false)?;
    table.set("DisplayScorebox", true)?;
    table.set("ErrorBar", "None")?;
    table.set("ErrorBarUp", false)?;
    table.set("ErrorBarMultiTick", false)?;
    table.set("ErrorBarTrim", "Off")?;
    table.set("ErrorBarCap", 5)?;
    table.set("HideEarlyDecentWayOffJudgments", false)?;
    table.set("HideEarlyDecentWayOffFlash", false)?;
    table.set("FlashMiss", true)?;
    table.set("FlashWayOff", false)?;
    table.set("FlashDecent", false)?;
    table.set("FlashGreat", false)?;
    table.set("FlashExcellent", false)?;
    table.set("FlashFantastic", false)?;
    table.set("ComboColors", "Glow")?;
    table.set("ComboMode", "FullCombo")?;
    table.set("TimerMode", "Time")?;
    table.set("JudgmentAnimation", "Default")?;
    table.set("RailBalance", "No")?;
    table.set("NoteFieldOffsetX", 0)?;
    table.set("NoteFieldOffsetY", 0)?;
    table.set("HeldGraphic", "None")?;
    table.set("NoteSkin", player.noteskin_name.as_str())?;
    table.set("NoteSkinVariant", "default")?;
    table.set("JudgmentGraphic", "None")?;
    table.set("ComboFont", "None")?;
    table.set("PlayerOptionsString", "")?;
    Ok(table)
}

fn create_sl_stages(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let stats = lua.create_table()?;
    stats.raw_set(1, create_sl_stage_stat(lua)?)?;
    table.set("PlayedThisGame", 0)?;
    table.set("Remaining", 1)?;
    table.set("Stats", stats)?;
    Ok(table)
}

fn create_sl_stage_stat(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let column_judgments = lua.create_table()?;
    for column in 1..=SONG_LUA_NOTE_COLUMNS {
        column_judgments.raw_set(column, create_sl_column_judgments(lua)?)?;
    }
    table.set("MusicRate", 1.0)?;
    table.set("DeathSecond", 0.0)?;
    table.set("worst_window", "W3")?;
    table.set("sequential_offsets", lua.create_table()?)?;
    table.set("column_judgments", column_judgments)?;
    table.set("ex_counts", create_sl_ex_counts(lua)?)?;
    Ok(table)
}

fn create_sl_column_judgments(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let early = lua.create_table()?;
    for name in ["W0", "W1", "W2", "W3", "W4", "W5", "Miss"] {
        table.set(name, 0)?;
        table.set(format!("{name}early"), 0)?;
        table.set(format!("{name}lf"), 0)?;
        table.set(format!("{name}rf"), 0)?;
        early.set(name, 0)?;
    }
    table.set("Early", early)?;
    table.set("MissBecauseHeld", 0)?;
    Ok(table)
}

fn create_sl_ex_counts(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in ["W0_total", "W1", "W2", "W3", "W4", "W5", "Miss"] {
        table.set(name, 0)?;
    }
    Ok(table)
}

fn create_sl_streams(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    init_sl_streams(lua, &table)?;
    Ok(table)
}

fn init_sl_streams(lua: &Lua, table: &Table) -> mlua::Result<()> {
    for name in [
        "NotesPerMeasure",
        "EquallySpacedPerMeasure",
        "NPSperMeasure",
        "ColumnCues",
    ] {
        if !matches!(table.get::<Value>(name)?, Value::Table(_)) {
            table.set(name, lua.create_table()?)?;
        }
    }
    for name in [
        "PeakNPS",
        "Crossovers",
        "Footswitches",
        "Sideswitches",
        "Jacks",
        "Brackets",
    ] {
        if matches!(table.get::<Value>(name)?, Value::Nil) {
            table.set(name, 0.0)?;
        }
    }
    for name in ["Hash", "Filename", "StepsType", "Difficulty", "Description"] {
        if matches!(table.get::<Value>(name)?, Value::Nil) {
            table.set(name, "")?;
        }
    }
    Ok(())
}

fn create_sl_high_scores(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("EnteringName", false)?;
    Ok(table)
}

fn create_sl_menu_timer(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in [
        "ScreenGrooveStatsLogin",
        "ScreenNameEntry",
        "ScreenPlayerOptions",
        "ScreenSelectMusic",
        "ScreenSelectMusicCasual",
        "ScreenEvaluation",
        "ScreenEvaluationNonstop",
        "ScreenEvaluationSummary",
    ] {
        table.set(name, 0)?;
    }
    Ok(table)
}

fn create_sl_screen_after(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("PlayAgain", "ScreenSelectMusic")?;
    table.set("PlayerOptions", "ScreenGameplay")?;
    table.set("PlayerOptions2", "ScreenGameplay")?;
    table.set("PlayerOptions3", "ScreenGameplay")?;
    table.set("PlayerOptions4", "ScreenGameplay")?;
    Ok(table)
}

fn create_sl_judgment_colors(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Casual", create_color_array(lua, SL_JUDGMENT_COLORS)?)?;
    table.set("ITG", create_color_array(lua, SL_JUDGMENT_COLORS)?)?;
    table.set("FA+", create_color_array(lua, SL_FA_PLUS_COLORS)?)?;
    Ok(table)
}

fn create_sl_groovestats(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("IsConnected", false)?;
    table.set("GetScores", false)?;
    table.set("Leaderboard", false)?;
    table.set("AutoSubmit", false)?;
    table.set("ChartHashVersion", "2")?;
    table.set("RequestCache", lua.create_table()?)?;
    table.set("UnlocksCache", lua.create_table()?)?;
    Ok(table)
}

fn create_sl_arrowcloud(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Enabled", false)?;
    table.set("BaseURL", "https://api.arrowcloud.dance")?;
    table.set("RequestTimeout", 5)?;
    Ok(table)
}

fn create_sl_srpg9(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Colors", create_string_array(lua, SL_DECORATIVE_COLORS)?)?;
    table.set("TextColor", "#ffffff")?;
    table.set(
        "GetLogo",
        lua.create_function(|_, _args: MultiValue| Ok("Logo.png"))?,
    )?;
    table.set(
        "MaybeRandomizeColor",
        lua.create_function(|_, _args: MultiValue| Ok(()))?,
    )?;
    Ok(table)
}

fn create_sl_mode_table(
    lua: &Lua,
    create: fn(&Lua, &str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for mode in ["Casual", "ITG", "FA+"] {
        table.set(mode, create(lua, mode)?)?;
    }
    Ok(table)
}

fn create_sl_preferences(lua: &Lua, mode: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("TimingWindowAdd", 0.0015)?;
    table.set("RegenComboAfterMiss", if mode == "Casual" { 0 } else { 5 })?;
    table.set(
        "MaxRegenComboAfterMiss",
        if mode == "Casual" { 0 } else { 10 },
    )?;
    table.set(
        "MinTNSToHideNotes",
        if mode == "FA+" {
            "TapNoteScore_W4"
        } else {
            "TapNoteScore_W3"
        },
    )?;
    table.set("MinTNSToScoreNotes", "TapNoteScore_None")?;
    table.set("HarshHotLifePenalty", true)?;
    table.set("PercentageScoring", true)?;
    table.set("AllowW1", "AllowW1_Everywhere")?;
    table.set("SubSortByNumSteps", true)?;
    let w1 = if mode == "FA+" { 0.0135 } else { 0.0215 };
    let w2 = if mode == "FA+" { 0.0215 } else { 0.043 };
    let w3 = if mode == "FA+" { 0.043 } else { 0.102 };
    table.set("TimingWindowSecondsW1", w1)?;
    table.set("TimingWindowSecondsW2", w2)?;
    table.set("TimingWindowSecondsW3", w3)?;
    table.set(
        "TimingWindowSecondsW4",
        if mode == "Casual" { 0.102 } else { 0.135 },
    )?;
    table.set(
        "TimingWindowSecondsW5",
        if mode == "Casual" { 0.102 } else { 0.18 },
    )?;
    table.set("TimingWindowSecondsHold", 0.32)?;
    table.set("TimingWindowSecondsMine", 0.07)?;
    table.set("TimingWindowSecondsRoll", 0.35)?;
    Ok(table)
}

fn create_sl_metrics(lua: &Lua, mode: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let fa_plus = mode == "FA+";
    for (name, value) in [
        ("PercentScoreWeightW1", 3),
        ("PercentScoreWeightW2", if fa_plus { 3 } else { 2 }),
        ("PercentScoreWeightW3", 1),
        ("PercentScoreWeightW4", 0),
        ("PercentScoreWeightW5", 0),
        ("PercentScoreWeightMiss", 0),
        ("PercentScoreWeightLetGo", 0),
        ("PercentScoreWeightHeld", 3),
        ("PercentScoreWeightHitMine", -1),
        ("PercentScoreWeightCheckpointHit", 0),
        ("GradeWeightW1", 3),
        ("GradeWeightW2", if fa_plus { 3 } else { 2 }),
        ("GradeWeightW3", 1),
        ("GradeWeightW4", 0),
        ("GradeWeightW5", 0),
        ("GradeWeightMiss", 0),
        ("GradeWeightLetGo", 0),
        ("GradeWeightHeld", 3),
        ("GradeWeightHitMine", -1),
        ("GradeWeightCheckpointHit", 0),
    ] {
        table.set(name, value)?;
    }
    for (name, value) in [
        ("LifePercentChangeW1", 0.008),
        ("LifePercentChangeW2", 0.008),
        ("LifePercentChangeW3", 0.004),
        ("LifePercentChangeW4", 0.0),
        ("LifePercentChangeW5", -0.04),
        ("LifePercentChangeMiss", -0.08),
        ("LifePercentChangeHitMine", -0.05),
        ("LifePercentChangeHeld", 0.008),
        ("LifePercentChangeLetGo", -0.08),
    ] {
        table.set(name, value)?;
    }
    Ok(table)
}

fn create_string_array(lua: &Lua, values: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, *value)?;
    }
    Ok(table)
}

fn create_owned_string_array(lua: &Lua, values: &[String]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, value.as_str())?;
    }
    Ok(table)
}

fn create_bool_array(lua: &Lua, values: &[bool]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, *value)?;
    }
    Ok(table)
}

fn create_color_array(lua: &Lua, values: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(
            index + 1,
            make_color_table(lua, parse_color_text(value).unwrap_or([1.0, 1.0, 1.0, 1.0]))?,
        )?;
    }
    Ok(table)
}

fn speedmod_parts(speedmod: SongLuaSpeedMod) -> (&'static str, f32) {
    match speedmod {
        SongLuaSpeedMod::X(value) => ("X", value),
        SongLuaSpeedMod::C(value) => ("C", value),
        SongLuaSpeedMod::M(value) => ("M", value),
        SongLuaSpeedMod::A(value) => ("A", value),
    }
}

fn player_short_name(player: usize) -> &'static str {
    match player {
        0 => "P1",
        1 => "P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}

fn song_music_rate_value(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

fn install_theme_color_helpers(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    globals.set(
        "GetHexColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(index) = args
                .get(0)
                .cloned()
                .and_then(read_f32)
                .map(|value| value.trunc() as i64)
            else {
                return Ok(make_color_table(lua, [1.0, 1.0, 1.0, 1.0])?);
            };
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            let diff_palette = args.get(2).cloned().and_then(read_string);
            let itg_diff = diff_palette
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("ITG"));
            let palette = match diff_palette.as_deref() {
                Some(value) if value.eq_ignore_ascii_case("ITG") => ITG_DIFF_COLORS,
                Some(value) if value.eq_ignore_ascii_case("DDR") => DDR_DIFF_COLORS,
                _ if decorative => SL_DECORATIVE_COLORS,
                _ => SL_COLORS,
            };
            let color = palette_color(index, palette);
            make_color_table(
                lua,
                if itg_diff && !decorative {
                    tone_color(color, 1.25)
                } else {
                    color
                },
            )
        })?,
    )?;
    globals.set(
        "GetCurrentColor",
        lua.create_function(|lua, args: MultiValue| {
            let decorative = args.get(0).cloned().and_then(read_boolish).unwrap_or(false);
            make_color_table(
                lua,
                palette_color(
                    SONG_LUA_ACTIVE_COLOR_INDEX,
                    if decorative {
                        SL_DECORATIVE_COLORS
                    } else {
                        SL_COLORS
                    },
                ),
            )
        })?,
    )?;
    globals.set(
        "PlayerColor",
        lua.create_function(|lua, args: MultiValue| {
            let player = args.get(0).and_then(player_index_from_value);
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            let index = match player {
                Some(0) => SONG_LUA_ACTIVE_COLOR_INDEX,
                Some(1) => SONG_LUA_ACTIVE_COLOR_INDEX - 2,
                _ => return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]),
            };
            make_color_table(
                lua,
                palette_color(
                    index,
                    if decorative {
                        SL_DECORATIVE_COLORS
                    } else {
                        SL_COLORS
                    },
                ),
            )
        })?,
    )?;
    globals.set(
        "PlayerScoreColor",
        lua.create_function(|lua, args: MultiValue| {
            let player = args.get(0).and_then(player_index_from_value);
            let index = match player {
                Some(0) => SONG_LUA_ACTIVE_COLOR_INDEX,
                Some(1) => SONG_LUA_ACTIVE_COLOR_INDEX - 2,
                _ => return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]),
            };
            make_color_table(lua, palette_color(index, SL_COLORS))
        })?,
    )?;
    globals.set(
        "PlayerDarkColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = match args.get(0).and_then(player_index_from_value) {
                Some(0) => parse_color_text("#da4453").unwrap_or([1.0, 1.0, 1.0, 1.0]),
                Some(1) => parse_color_text("#4a89dc").unwrap_or([1.0, 1.0, 1.0, 1.0]),
                _ => [1.0, 1.0, 1.0, 1.0],
            };
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "DifficultyColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(difficulty) = args.get(0).cloned().and_then(difficulty_index_from_value)
            else {
                return make_color_table(lua, parse_color_text("#B4B7BA").unwrap_or([1.0; 4]));
            };
            if difficulty == 5 {
                return make_color_table(lua, parse_color_text("#B4B7BA").unwrap_or([1.0; 4]));
            }
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            make_color_table(
                lua,
                palette_color(
                    SONG_LUA_ACTIVE_COLOR_INDEX + difficulty - 4,
                    if decorative {
                        SL_DECORATIVE_COLORS
                    } else {
                        SL_COLORS
                    },
                ),
            )
        })?,
    )?;
    globals.set(
        "CustomDifficultyToColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(custom_difficulty_color)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "CustomDifficultyToDarkColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(custom_difficulty_color)
                .map(|color| tone_color(color, 0.5))
                .unwrap_or([0.5, 0.5, 0.5, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "CustomDifficultyToLightColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(custom_difficulty_color)
                .map(|color| {
                    [
                        scale_value(color[0], 0.0, 1.0, 0.5, 1.0),
                        scale_value(color[1], 0.0, 1.0, 0.5, 1.0),
                        scale_value(color[2], 0.0, 1.0, 0.5, 1.0),
                        color[3],
                    ]
                })
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "StepsOrTrailToColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(steps_or_trail_color)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "StageToColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(stage_color)
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "StageToStrokeColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(stage_color)
                .map(|color| tone_color(color, 0.5))
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "JudgmentLineToStrokeColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(judgment_line_color)
                .map(|color| tone_color(color, 0.5))
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "JudgmentLineToColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(judgment_line_color)
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    for (name, factor) in [
        ("LightenColor", 1.25_f32),
        ("ColorLightTone", 1.5_f32),
        ("ColorMidTone", 1.0_f32 / 1.5_f32),
        ("ColorDarkTone", 0.5_f32),
    ] {
        globals.set(
            name,
            lua.create_function(move |lua, args: MultiValue| {
                let color = args
                    .get(0)
                    .cloned()
                    .and_then(read_color_value)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                make_color_table(lua, tone_color(color, factor))
            })?,
        )?;
    }
    globals.set(
        "HasAlpha",
        lua.create_function(|_, args: MultiValue| {
            Ok(args
                .get(0)
                .cloned()
                .and_then(read_color_value)
                .map(|color| color[3])
                .unwrap_or(1.0))
        })?,
    )?;
    globals.set(
        "ColorToHex",
        lua.create_function(|_, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(read_color_value)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            Ok(color_to_hex(color))
        })?,
    )?;
    globals.set(
        "BoostColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(read_color_value)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let boost = args.get(1).cloned().and_then(read_f32).unwrap_or(1.0);
            make_color_table(lua, tone_color(color, boost))
        })?,
    )?;
    globals.set(
        "BlendColors",
        lua.create_function(|lua, args: MultiValue| {
            let first = args
                .get(0)
                .cloned()
                .and_then(read_color_value)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let second = args
                .get(1)
                .cloned()
                .and_then(read_color_value)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            make_color_table(lua, blend_color(first, second))
        })?,
    )?;
    Ok(())
}

fn palette_color(index: i64, palette: &[&str]) -> [f32; 4] {
    if palette.is_empty() {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let wrapped = (index - 1).rem_euclid(palette.len() as i64) as usize;
    parse_color_text(palette[wrapped]).unwrap_or([1.0, 1.0, 1.0, 1.0])
}

fn difficulty_index_from_value(value: Value) -> Option<i64> {
    match value {
        Value::Integer(value) => Some(value),
        Value::Number(value) if value.is_finite() => Some(value.trunc() as i64),
        Value::String(text) => match text.to_str().ok()?.as_ref() {
            "Beginner" | "Difficulty_Beginner" => Some(0),
            "Easy" | "Difficulty_Easy" => Some(1),
            "Medium" | "Difficulty_Medium" => Some(2),
            "Hard" | "Difficulty_Hard" => Some(3),
            "Challenge" | "Difficulty_Challenge" => Some(4),
            "Edit" | "Difficulty_Edit" => Some(5),
            _ => None,
        },
        _ => None,
    }
}

fn custom_difficulty_color(value: Value) -> Option<[f32; 4]> {
    let name = read_string(value)?;
    let hex = match name.as_str() {
        "Beginner" | "Difficulty_Beginner" => "#ff32f8",
        "Easy" | "Difficulty_Easy" | "Freestyle" => "#2cff00",
        "Medium" | "Difficulty_Medium" | "HalfDouble" => "#fee600",
        "Hard" | "Difficulty_Hard" | "Crazy" => "#ff2f39",
        "Challenge" | "Difficulty_Challenge" | "Nightmare" => "#1cd8ff",
        "Edit" | "Difficulty_Edit" => "#cccccc",
        "Couple" | "Difficulty_Couple" => "#ed0972",
        "Routine" | "Difficulty_Routine" => "#ff9a00",
        _ => return None,
    };
    parse_color_text(hex)
}

fn steps_or_trail_color(value: Value) -> Option<[f32; 4]> {
    if let Some(color) = custom_difficulty_color(value.clone()) {
        return Some(color);
    }
    let table = match value {
        Value::Table(table) => table,
        _ => return None,
    };
    let Value::Function(get_difficulty) = table.get::<Value>("GetDifficulty").ok()? else {
        return None;
    };
    custom_difficulty_color(get_difficulty.call::<Value>(table).ok()?)
}

fn stage_color(value: Value) -> Option<[f32; 4]> {
    let name = read_string(value)?;
    let hex = match name.as_str() {
        "Stage_1st" => "#00ffc7",
        "Stage_2nd" => "#58ff00",
        "Stage_3rd" => "#f400ff",
        "Stage_4th" => "#00ffda",
        "Stage_5th" => "#ed00ff",
        "Stage_6th" => "#73ff00",
        "Stage_Next" => "#73ff00",
        "Stage_Final" | "Stage_Extra2" => "#ff0707",
        "Stage_Extra1" => "#fafa00",
        "Stage_Nonstop" | "Stage_Oni" | "Stage_Endless" | "Stage_Event" | "Stage_Demo" => "#ffffff",
        _ => return None,
    };
    parse_color_text(hex)
}

fn judgment_line_color(value: Value) -> Option<[f32; 4]> {
    let name = read_string(value)?;
    let hex = match name.as_str() {
        "JudgmentLine_W1" => "#bfeaff",
        "JudgmentLine_W2" => "#fff568",
        "JudgmentLine_W3" => "#a4ff00",
        "JudgmentLine_W4" => "#34bfff",
        "JudgmentLine_W5" => "#e44dff",
        "JudgmentLine_Held" => "#ffffff",
        "JudgmentLine_Miss" => "#ff3c3c",
        "JudgmentLine_MaxCombo" => "#ffc600",
        _ => return None,
    };
    parse_color_text(hex)
}

fn tone_color(color: [f32; 4], factor: f32) -> [f32; 4] {
    [
        color[0] * factor,
        color[1] * factor,
        color[2] * factor,
        color[3],
    ]
}

fn blend_color(first: [f32; 4], second: [f32; 4]) -> [f32; 4] {
    [
        0.5 * (first[0] + second[0]),
        0.5 * (first[1] + second[1]),
        0.5 * (first[2] + second[2]),
        0.5 * (first[3] + second[3]),
    ]
}

fn color_to_hex(color: [f32; 4]) -> String {
    let component = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as u8;
    format!(
        "{:02X}{:02X}{:02X}{:02X}",
        component(color[0]),
        component(color[1]),
        component(color[2]),
        component(color[3])
    )
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

fn create_song_options_table(lua: &Lua, music_rate: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("__songlua_music_rate", music_rate.max(0.0))?;
    table.set(
        "MusicRate",
        lua.create_function(move |_, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(1.0_f32);
            };
            if let Some(rate) = method_arg(&args, 0).cloned().and_then(read_f32) {
                owner.set("__songlua_music_rate", rate.max(0.0))?;
                return Ok(rate.max(0.0));
            }
            Ok(owner
                .get::<Option<f32>>("__songlua_music_rate")?
                .unwrap_or(1.0_f32))
        })?,
    )?;
    Ok(table)
}

fn format_song_options_text(music_rate: f32) -> String {
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    format!("{rate}xMusic")
}

struct PlayerLuaTables {
    player_states: [Table; LUA_PLAYERS],
    steps: [Table; LUA_PLAYERS],
}

struct TopScreenLuaTables {
    top_screen: Table,
    players: [Table; LUA_PLAYERS],
}

#[inline(always)]
fn song_lua_column_x(column_index: usize) -> f32 {
    SONG_LUA_COLUMN_X.get(column_index).copied().unwrap_or(0.0)
}

fn create_arrow_effects_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetYOffset",
        lua.create_function(|_, args: MultiValue| {
            Ok(args
                .get(2)
                .cloned()
                .and_then(read_f32)
                .map(|beat| -64.0 * beat)
                .unwrap_or(0.0_f32))
        })?,
    )?;
    table.set(
        "GetYPos",
        lua.create_function(|_, args: MultiValue| {
            Ok(args.get(2).cloned().and_then(read_f32).unwrap_or(0.0_f32))
        })?,
    )?;
    table.set(
        "GetXPos",
        lua.create_function(|_, args: MultiValue| {
            let column_index = args
                .get(1)
                .cloned()
                .and_then(read_f32)
                .map(|value| value as isize - 1)
                .filter(|value| *value >= 0)
                .map(|value| value as usize)
                .unwrap_or(0);
            Ok(song_lua_column_x(column_index))
        })?,
    )?;
    table.set(
        "GetZPos",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationX",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationY",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationZ",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetZoom",
        lua.create_function(|_, _args: MultiValue| Ok(1.0_f32))?,
    )?;
    table.set(
        "GetAlpha",
        lua.create_function(|_, _args: MultiValue| Ok(1.0_f32))?,
    )?;
    table.set(
        "GetGlow",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(table)
}

fn actor_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(children) = actor.get::<Option<Table>>("__songlua_children")? {
        return Ok(children);
    }
    let children = lua.create_table()?;
    actor.set("__songlua_children", children.clone())?;
    Ok(children)
}

fn actor_named_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let children = lua.create_table()?;
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (key, value) = pair?;
        children.set(key, value)?;
    }
    merge_actor_sequence_children(lua, actor, &children)?;
    Ok(children)
}

fn actor_direct_children(lua: &Lua, actor: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        push_unique_actor_child(&mut out, &mut seen, child);
    }
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (_, value) = pair?;
        let Value::Table(child) = value else {
            continue;
        };
        if child
            .get::<Option<bool>>("__songlua_child_group")?
            .unwrap_or(false)
        {
            for group_value in child.sequence_values::<Value>() {
                if let Value::Table(group_child) = group_value? {
                    push_unique_actor_child(&mut out, &mut seen, group_child);
                }
            }
        } else {
            push_unique_actor_child(&mut out, &mut seen, child);
        }
    }
    Ok(out)
}

fn actor_child_at(lua: &Lua, actor: &Table, index: usize) -> mlua::Result<Value> {
    Ok(actor_direct_children(lua, actor)?
        .into_iter()
        .nth(index)
        .map_or(Value::Nil, Value::Table))
}

fn read_child_index(value: &Value) -> Option<usize> {
    match value {
        Value::Integer(value) if *value >= 0 => Some(*value as usize),
        Value::Number(value) if value.is_finite() && *value >= 0.0 => Some(*value as usize),
        _ => None,
    }
}

fn push_unique_actor_child(out: &mut Vec<Table>, seen: &mut Vec<usize>, child: Table) {
    let ptr = child.to_pointer() as usize;
    if seen.contains(&ptr) {
        return;
    }
    seen.push(ptr);
    out.push(child);
}

fn merge_actor_sequence_children(lua: &Lua, actor: &Table, children: &Table) -> mlua::Result<()> {
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        let Some(name) = child.get::<Option<String>>("Name")? else {
            continue;
        };
        if name.trim().is_empty() {
            continue;
        }
        match children.get::<Option<Value>>(name.as_str())? {
            Some(Value::Table(group))
                if group
                    .get::<Option<bool>>("__songlua_child_group")?
                    .unwrap_or(false) =>
            {
                group.raw_set(group.raw_len() + 1, child)?;
            }
            Some(Value::Table(existing)) => {
                let group = lua.create_table()?;
                group.set("__songlua_child_group", true)?;
                group.raw_set(1, existing)?;
                group.raw_set(2, child)?;
                children.set(name.as_str(), group)?;
            }
            Some(_) => {}
            None => {
                children.set(name.as_str(), child)?;
            }
        }
    }
    Ok(())
}

fn actor_wrappers(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        return Ok(wrappers);
    }
    let wrappers = lua.create_table()?;
    actor.set("__songlua_wrappers", wrappers.clone())?;
    Ok(wrappers)
}

fn copy_dummy_actor_tags(from: &Table, into: &Table) -> mlua::Result<()> {
    if let Some(player_index) = from.get::<Option<i64>>("__songlua_player_index")? {
        into.set("__songlua_player_index", player_index)?;
    }
    if let Some(child_name) = from.get::<Option<String>>("__songlua_player_child_name")? {
        into.set("__songlua_player_child_name", child_name)?;
    }
    if let Some(child_name) = from.get::<Option<String>>("__songlua_top_screen_child_name")? {
        into.set("__songlua_top_screen_child_name", child_name)?;
    }
    for key in [
        "__songlua_main_title",
        "__songlua_bpm_text",
        "__songlua_steps_text_1",
        "__songlua_steps_text_2",
    ] {
        if let Some(value) = from.get::<Option<String>>(key)? {
            into.set(key, value)?;
        }
    }
    Ok(())
}

fn create_named_child_actor(lua: &Lua, parent: &Table, name: &str) -> mlua::Result<Table> {
    let parent_type = parent.get::<Option<String>>("__songlua_actor_type")?;
    let player_index = parent.get::<Option<i64>>("__songlua_player_index")?;
    let child = if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
        && name.eq_ignore_ascii_case("NoteField")
        && let Some(player_index) = player_index
    {
        create_note_field_actor(lua, player_index as usize)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && name.eq_ignore_ascii_case("Timer")
    {
        create_screen_timer_actor(lua)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && let Some(player_index) = top_screen_score_index(name)
    {
        create_top_screen_score_actor(lua, player_index)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && let Some(child) = create_top_screen_theme_actor(lua, parent, name)?
    {
        child
    } else if parent
        .get::<Option<String>>("__songlua_top_screen_child_name")?
        .as_deref()
        .is_some_and(|child_name| child_name.eq_ignore_ascii_case("Underlay"))
        && let Some(child) = create_underlay_theme_actor(lua, parent, name)?
    {
        child
    } else {
        create_dummy_actor(lua, "ChildActor")?
    };
    copy_dummy_actor_tags(parent, &child)?;
    child.set("__songlua_parent", parent.clone())?;
    if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
    {
        child.set("__songlua_player_child_name", name)?;
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
    {
        child.set("__songlua_top_screen_child_name", name)?;
    }
    Ok(child)
}

fn create_named_actor(lua: &Lua, actor_type: &'static str, name: &str) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, actor_type)?;
    actor.set("Name", name)?;
    Ok(actor)
}

fn create_named_text_actor(
    lua: &Lua,
    actor_type: &'static str,
    name: &str,
    text: String,
) -> mlua::Result<Table> {
    let actor = create_named_actor(lua, actor_type, name)?;
    actor.set("Text", text)?;
    Ok(actor)
}

fn create_top_screen_theme_actor(
    lua: &Lua,
    parent: &Table,
    name: &str,
) -> mlua::Result<Option<Table>> {
    if name.eq_ignore_ascii_case("Underlay") || name.eq_ignore_ascii_case("Overlay") {
        let actor = create_named_actor(lua, "ActorFrame", name)?;
        if name.eq_ignore_ascii_case("Underlay") {
            copy_dummy_actor_tags(parent, &actor)?;
            actor.set("__songlua_top_screen_child_name", "Underlay")?;
            install_underlay_theme_children(lua, &actor)?;
        }
        return Ok(Some(actor));
    }
    if name.eq_ignore_ascii_case("BPMDisplay") {
        return Ok(Some(create_named_text_actor(
            lua,
            "BPMDisplay",
            name,
            parent
                .get::<Option<String>>("__songlua_bpm_text")?
                .unwrap_or_default(),
        )?));
    }
    if name.eq_ignore_ascii_case("SongTitle") {
        return Ok(Some(create_named_text_actor(
            lua,
            "BitmapText",
            name,
            parent
                .get::<Option<String>>("__songlua_main_title")?
                .unwrap_or_default(),
        )?));
    }
    if name.eq_ignore_ascii_case("StageDisplay") {
        return Ok(Some(create_named_text_actor(
            lua,
            "BitmapText",
            name,
            "Stage 1".to_string(),
        )?));
    }
    if let Some(player_index) = top_screen_steps_display_index(name) {
        return Ok(Some(create_named_text_actor(
            lua,
            "StepsDisplay",
            name,
            top_screen_steps_text(parent, player_index)?,
        )?));
    }
    if top_screen_song_meter_display_index(name).is_some() {
        return Ok(Some(create_top_screen_song_meter_display_actor(lua, name)?));
    }
    if name.eq_ignore_ascii_case("LifeFrame")
        || name.eq_ignore_ascii_case("ScoreFrame")
        || name.eq_ignore_ascii_case("Lyrics")
        || name.eq_ignore_ascii_case("SongBackground")
        || name.eq_ignore_ascii_case("SongForeground")
    {
        return Ok(Some(create_named_actor(lua, "ActorFrame", name)?));
    }
    if top_screen_life_meter_bar_index(name).is_some() {
        return Ok(Some(create_named_actor(lua, "LifeMeterBar", name)?));
    }
    Ok(None)
}

fn create_underlay_theme_actor(
    lua: &Lua,
    parent: &Table,
    name: &str,
) -> mlua::Result<Option<Table>> {
    if let Some(player_index) = underlay_score_index(name) {
        return Ok(Some(create_named_text_actor(
            lua,
            "BitmapText",
            underlay_score_name(player_index),
            "0.00%".to_string(),
        )?));
    }
    if name.eq_ignore_ascii_case("SongMeter") {
        let actor = create_named_actor(lua, "ActorFrame", name)?;
        let title = create_named_text_actor(
            lua,
            "BitmapText",
            "SongTitle",
            parent
                .get::<Option<String>>("__songlua_main_title")?
                .unwrap_or_default(),
        )?;
        title.set("__songlua_parent", actor.clone())?;
        actor_children(lua, &actor)?.set("SongTitle", title)?;
        return Ok(Some(actor));
    }
    if top_screen_step_stats_pane_index(name).is_some() {
        return Ok(Some(create_named_actor(lua, "ActorFrame", name)?));
    }
    if top_screen_danger_index(name).is_some() || name.eq_ignore_ascii_case("Header") {
        return Ok(Some(create_named_actor(lua, "Sprite", name)?));
    }
    Ok(None)
}

fn create_top_screen_song_meter_display_actor(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let actor = create_named_actor(lua, "SongMeterDisplay", name)?;
    actor.set("StreamWidth", 0.0_f32)?;
    actor.set("__songlua_stream_width", 0.0_f32)?;
    let stream = create_named_actor(lua, "Quad", "Stream")?;
    stream.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set("Stream", stream)?;
    Ok(actor)
}

const TOP_SCREEN_THEME_CHILD_NAMES: &[&str] = &[
    "Underlay",
    "Overlay",
    "BPMDisplay",
    "LifeFrame",
    "ScoreFrame",
    "Lyrics",
    "SongBackground",
    "SongForeground",
    "StageDisplay",
    "SongTitle",
    "ScoreP1",
    "ScoreP2",
    "SongMeterDisplayP1",
    "SongMeterDisplayP2",
    "StepsDisplayP1",
    "StepsDisplayP2",
    "LifeMeterBarP1",
    "LifeMeterBarP2",
];

const UNDERLAY_THEME_CHILD_NAMES: &[&str] = &[
    "Header",
    "StepStatsPaneP1",
    "StepStatsPaneP2",
    "DangerP1",
    "DangerP2",
    "P1Score",
    "P2Score",
    "SongMeter",
];

fn install_top_screen_theme_children(lua: &Lua, top_screen: &Table) -> mlua::Result<()> {
    install_named_theme_children(lua, top_screen, TOP_SCREEN_THEME_CHILD_NAMES)
}

fn install_underlay_theme_children(lua: &Lua, underlay: &Table) -> mlua::Result<()> {
    install_named_theme_children(lua, underlay, UNDERLAY_THEME_CHILD_NAMES)
}

fn install_named_theme_children(lua: &Lua, parent: &Table, names: &[&str]) -> mlua::Result<()> {
    let children = actor_children(lua, parent)?;
    for &name in names {
        if children.get::<Option<Table>>(name)?.is_some() {
            continue;
        }
        let child = create_named_child_actor(lua, parent, name)?;
        children.set(name, child)?;
    }
    Ok(())
}

fn create_screen_timer_actor(lua: &Lua) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "Timer")?;
    actor.set(
        "GetSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(actor)
}

fn create_top_screen_score_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "ActorFrame")?;
    actor.set("Name", top_screen_score_name(player_index))?;
    let percent = create_score_display_percent_actor(lua, player_index)?;
    percent.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set("ScoreDisplayPercentage Percent", percent)?;
    Ok(actor)
}

fn create_score_display_percent_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "PercentageDisplay")?;
    actor.set("Name", "ScoreDisplayPercentage Percent")?;
    let text = create_score_percent_text_actor(lua, player_index)?;
    text.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set(top_screen_score_percent_name(player_index), text)?;
    Ok(actor)
}

fn create_score_percent_text_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "BitmapText")?;
    actor.set("Name", top_screen_score_percent_name(player_index))?;
    actor.set("Text", "0.00%")?;
    Ok(actor)
}

fn create_life_meter_table(lua: &Lua, name: &'static str) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "LifeMeter")?;
    actor.set("Name", name)?;
    actor.set("__songlua_life", SONG_LUA_INITIAL_LIFE)?;
    actor.set(
        "GetLife",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE))
            }
        })?,
    )?;
    actor.set(
        "IsFailing",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    <= 0.0)
            }
        })?,
    )?;
    actor.set(
        "IsHot",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    >= 1.0)
            }
        })?,
    )?;
    actor.set(
        "IsInDanger",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    < SONG_LUA_DANGER_LIFE)
            }
        })?,
    )?;
    Ok(actor)
}

fn create_note_field_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteField")?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_player_child_name", "NoteField")?;
    actor.set("__songlua_beat_bars", false)?;
    actor.set("__songlua_beat_bars_alpha", lua.create_table()?)?;
    actor.set(
        "SetBeatBars",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0) {
                    actor.set("__songlua_beat_bars", truthy(value))?;
                    note_song_lua_side_effect(lua)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetBeatBarsAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let alpha = lua.create_table()?;
                for index in 0..4 {
                    let value = method_arg(&args, index)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0);
                    alpha.raw_set(index + 1, value)?;
                }
                actor.set("__songlua_beat_bars_alpha", alpha)?;
                note_song_lua_side_effect(lua)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("__songlua_state_x", 0.0_f32)?;
    actor.set(
        "__songlua_state_y",
        0.5 * (THEME_RECEPTOR_Y_STD + THEME_RECEPTOR_Y_REV),
    )?;
    actor.set("__songlua_state_z", 0.0_f32)?;
    Ok(actor)
}

fn note_field_column_actors(lua: &Lua, note_field: &Table) -> mlua::Result<Table> {
    if let Some(columns) = note_field.get::<Option<Table>>("__songlua_note_columns")? {
        return Ok(columns);
    }
    let columns = lua.create_table()?;
    let player_index = note_field
        .get::<Option<i64>>("__songlua_player_index")?
        .unwrap_or(0) as usize;
    for column_index in 0..SONG_LUA_NOTE_COLUMNS {
        let column = create_note_column_actor(lua, note_field, player_index, column_index)?;
        columns.raw_set(column_index + 1, column)?;
    }
    note_field.set("__songlua_note_columns", columns.clone())?;
    Ok(columns)
}

fn create_note_column_actor(
    lua: &Lua,
    note_field: &Table,
    player_index: usize,
    column_index: usize,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteColumnRenderer")?;
    actor.set("__songlua_parent", note_field.clone())?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_state_x", song_lua_column_x(column_index))?;
    actor.set("__songlua_state_y", 0.0_f32)?;
    actor.set("__songlua_state_z", 0.0_f32)?;

    let pos_handler = create_note_column_spline_handler(lua)?;
    let rot_handler = create_note_column_spline_handler(lua)?;
    let zoom_handler = create_note_column_spline_handler(lua)?;
    actor.set(
        "GetPosHandler",
        lua.create_function(move |_, _args: MultiValue| Ok(pos_handler.clone()))?,
    )?;
    actor.set(
        "GetRotHandler",
        lua.create_function(move |_, _args: MultiValue| Ok(rot_handler.clone()))?,
    )?;
    actor.set(
        "GetZoomHandler",
        lua.create_function(move |_, _args: MultiValue| Ok(zoom_handler.clone()))?,
    )?;
    Ok(actor)
}

fn create_note_column_spline_handler(lua: &Lua) -> mlua::Result<Table> {
    let handler = lua.create_table()?;
    let spline = create_cubic_spline_table(lua)?;
    handler.set("__songlua_spline_mode", "NoteColumnSplineMode_Offset")?;
    handler.set("__songlua_subtract_song_beat", false)?;
    handler.set("__songlua_receptor_t", 0.0_f32)?;
    handler.set("__songlua_beats_per_t", 1.0_f32)?;
    handler.set(
        "GetSpline",
        lua.create_function(move |_, _args: MultiValue| Ok(spline.clone()))?,
    )?;
    handler.set(
        "SetSplineMode",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(mode) = args.get(1).cloned().and_then(read_string) {
                    handler.set("__songlua_spline_mode", mode)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetSubtractSongBeat",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                handler.set(
                    "__songlua_subtract_song_beat",
                    args.get(1).is_some_and(truthy),
                )?;
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetReceptorT",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    handler.set("__songlua_receptor_t", value)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetBeatsPerT",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    handler.set("__songlua_beats_per_t", value)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    Ok(handler)
}

fn create_cubic_spline_table(lua: &Lua) -> mlua::Result<Table> {
    let spline = lua.create_table()?;
    spline.set("__songlua_spline_size", 0_i64)?;
    spline.set("__songlua_spline_points", lua.create_table()?)?;
    spline.set(
        "SetSize",
        lua.create_function({
            let spline = spline.clone();
            move |_, args: MultiValue| {
                if let Some(size) = args.get(1).cloned().and_then(read_f32) {
                    spline.set("__songlua_spline_size", size.max(0.0).round() as i64)?;
                }
                Ok(spline.clone())
            }
        })?,
    )?;
    spline.set(
        "SetPoint",
        lua.create_function({
            let spline = spline.clone();
            move |lua, args: MultiValue| {
                let Some(index) = args
                    .get(1)
                    .cloned()
                    .and_then(read_f32)
                    .map(|value| value.max(1.0).round() as i64)
                else {
                    return Ok(spline.clone());
                };
                let points = spline.get::<Table>("__songlua_spline_points")?;
                match args.get(2) {
                    Some(Value::Table(point)) => {
                        points.raw_set(index, point.clone())?;
                    }
                    _ => {
                        let point = lua.create_table()?;
                        point.raw_set(1, 0.0_f32)?;
                        point.raw_set(2, 0.0_f32)?;
                        point.raw_set(3, 0.0_f32)?;
                        points.raw_set(index, point)?;
                    }
                }
                Ok(spline.clone())
            }
        })?,
    )?;
    spline.set(
        "Solve",
        lua.create_function({
            let spline = spline.clone();
            move |_, _args: MultiValue| Ok(spline.clone())
        })?,
    )?;
    spline.set(
        "SetPolygonal",
        lua.create_function({
            let spline = spline.clone();
            move |_, _args: MultiValue| Ok(spline.clone())
        })?,
    )?;
    Ok(spline)
}

fn create_top_screen_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    current_sort_order: Table,
    current_song: Table,
) -> mlua::Result<TopScreenLuaTables> {
    let players = context.players.clone();
    let top_screen = create_dummy_actor(lua, "TopScreen")?;
    top_screen.set("Name", "ScreenGameplay")?;
    top_screen.set("__songlua_main_title", context.main_title.as_str())?;
    top_screen.set(
        "__songlua_bpm_text",
        display_bpms_text(context.song_display_bpms, song_music_rate(context)),
    )?;
    top_screen.set("__songlua_steps_text_1", players[0].difficulty.sm_name())?;
    top_screen.set("__songlua_steps_text_2", players[1].difficulty.sm_name())?;
    let top_screen_for_get_child = top_screen.clone();
    let life_meters = [
        create_life_meter_table(lua, "LifeP1")?,
        create_life_meter_table(lua, "LifeP2")?,
    ];
    let player_actors = [
        create_top_screen_player_actor(lua, players[0].clone(), 0)?,
        create_top_screen_player_actor(lua, players[1].clone(), 1)?,
    ];
    for player_actor in &player_actors {
        player_actor.set("__songlua_parent", top_screen.clone())?;
    }
    let nameless_children = lua.create_table()?;
    for (player_index, player_actor) in player_actors.iter().enumerate() {
        if players[player_index].enabled {
            nameless_children.raw_set(nameless_children.raw_len() + 1, player_actor.clone())?;
        }
    }
    for (player_index, life_meter) in life_meters.iter().enumerate() {
        life_meter.set("__songlua_parent", top_screen.clone())?;
        life_meter.set(
            "__songlua_top_screen_child_name",
            top_screen_life_meter_name(player_index),
        )?;
    }
    let children = actor_children(lua, &top_screen)?;
    children.set("", nameless_children)?;
    for (player_index, player_actor) in player_actors.iter().enumerate() {
        if players[player_index].enabled {
            children.set(top_screen_player_name(player_index), player_actor.clone())?;
        }
    }
    children.set("LifeP1", life_meters[0].clone())?;
    children.set("LifeP2", life_meters[1].clone())?;
    children.set("LifeMeter", life_meters[0].clone())?;
    install_top_screen_theme_children(lua, &top_screen)?;
    let player_actors_for_get_child = player_actors.clone();
    let life_meters_for_get_child = life_meters.clone();
    let players_for_get_child = players.clone();
    top_screen.set(
        "GetChild",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            if let Some(player_index) = top_screen_player_index(&name) {
                return if players_for_get_child[player_index].enabled {
                    Ok(Value::Table(
                        player_actors_for_get_child[player_index].clone(),
                    ))
                } else {
                    Ok(Value::Nil)
                };
            }
            if let Some(player_index) = top_screen_life_meter_index(&name) {
                return if players_for_get_child[player_index].enabled {
                    Ok(Value::Table(
                        life_meters_for_get_child[player_index].clone(),
                    ))
                } else {
                    Ok(Value::Nil)
                };
            }
            if name.eq_ignore_ascii_case("LifeMeter") {
                return Ok(Value::Table(life_meters_for_get_child[0].clone()));
            }
            let children = actor_children(lua, &top_screen_for_get_child)?;
            if let Some(child) = children.get::<Option<Table>>(name.as_str())? {
                return Ok(Value::Table(child));
            }
            let child = create_named_child_actor(lua, &top_screen_for_get_child, &name)?;
            children.set(name.as_str(), child.clone())?;
            Ok(Value::Table(child))
        })?,
    )?;
    let life_meters_for_get_life_meter = life_meters.clone();
    let players_for_get_life_meter = players.clone();
    top_screen.set(
        "GetLifeMeter",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player_index) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            if !players_for_get_life_meter[player_index].enabled {
                return Ok(Value::Nil);
            }
            Ok(Value::Table(
                life_meters_for_get_life_meter[player_index].clone(),
            ))
        })?,
    )?;
    top_screen.set(
        "StartTransitioningScreen",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "SetNextScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                let next_screen = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                top_screen.set("__songlua_next_screen_name", next_screen)?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "SetPrevScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                let prev_screen = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                top_screen.set("__songlua_prev_screen_name", prev_screen)?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "GetNextScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: MultiValue| {
                Ok(Value::String(
                    lua.create_string(
                        top_screen
                            .get::<Option<String>>("__songlua_next_screen_name")?
                            .unwrap_or_default(),
                    )?,
                ))
            }
        })?,
    )?;
    top_screen.set(
        "GetPrevScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: MultiValue| {
                Ok(Value::String(
                    lua.create_string(
                        top_screen
                            .get::<Option<String>>("__songlua_prev_screen_name")?
                            .unwrap_or_default(),
                    )?,
                ))
            }
        })?,
    )?;
    top_screen.set(
        "GetGoToOptions",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    top_screen.set(
        "GetEditState",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("EditState_Playing")?))
        })?,
    )?;
    top_screen.set(
        "GetCurrentRowIndex",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |_, _args: MultiValue| {
                Ok(top_screen
                    .get::<Option<i64>>("__songlua_current_row_index")?
                    .unwrap_or(0))
            }
        })?,
    )?;
    top_screen.set(
        "GetNumRows",
        lua.create_function(|_, _args: MultiValue| {
            Ok(SONG_LUA_TOP_SCREEN_OPTION_ROWS.len() as i64)
        })?,
    )?;
    top_screen.set(
        "GetOptionRow",
        lua.create_function(|lua, args: MultiValue| {
            let row_name = top_screen_option_row_name(method_arg(&args, 0).cloned());
            create_option_row_table(lua, &row_name)
        })?,
    )?;
    top_screen.set(
        "SetOptionRowIndex",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                if let Some(index) = method_arg(&args, 1)
                    .or_else(|| method_arg(&args, 0))
                    .cloned()
                    .and_then(read_i32_value)
                {
                    top_screen.set("__songlua_current_row_index", i64::from(index.max(0)))?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "RedrawOptions",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "IsPaused",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |_, _args: MultiValue| {
                Ok(top_screen
                    .get::<Option<bool>>("__songlua_paused")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    top_screen.set(
        "PauseGame",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                top_screen.set("__songlua_paused", method_arg(&args, 0).is_some_and(truthy))?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "AllAreOnLastRow",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    let music_wheel = create_music_wheel_table(lua, current_sort_order)?;
    top_screen.set(
        "GetMusicWheel",
        lua.create_function(move |_, _args: MultiValue| Ok(music_wheel.clone()))?,
    )?;
    top_screen.set(
        "GetNextCourseSong",
        lua.create_function(move |_, _args: MultiValue| {
            Ok(current_song
                .raw_get::<Option<Table>>(1)?
                .map_or(Value::Nil, Value::Table))
        })?,
    )?;
    for name in [
        "AddInputCallback",
        "RemoveInputCallback",
        "PostScreenMessage",
        "SetProfileIndex",
        "Cancel",
        "Continue",
        "Finish",
        "Load",
        "begin_backing_out",
    ] {
        top_screen.set(
            name,
            lua.create_function({
                let top_screen = top_screen.clone();
                move |lua, _args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    Ok(top_screen.clone())
                }
            })?,
        )?;
    }
    Ok(TopScreenLuaTables {
        top_screen,
        players: player_actors,
    })
}

fn create_music_wheel_table(lua: &Lua, current_sort_order: Table) -> mlua::Result<Table> {
    let wheel = lua.create_table()?;
    wheel.set(
        "IsLocked",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    wheel.set(
        "GetSelectedSection",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    wheel.set(
        "GetSelectedSong",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    wheel.set(
        "GetSelectedType",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("WheelItemDataType_Song")?))
        })?,
    )?;
    wheel.set(
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
    wheel.set(
        "ChangeSort",
        lua.create_function({
            let wheel = wheel.clone();
            let current_sort_order = current_sort_order.clone();
            move |lua, args: MultiValue| {
                if let Some(sort) = method_arg(&args, 0).cloned().and_then(read_string) {
                    current_sort_order.raw_set(1, sort)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    wheel.set(
        "SetOpenSection",
        lua.create_function({
            let wheel = wheel.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    wheel.set(
        "Move",
        lua.create_function({
            let wheel = wheel.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    Ok(wheel)
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
const SONG_LUA_TOP_SCREEN_OPTION_ROWS: &[&str] = &[
    "SpeedModType",
    "SpeedMod",
    "Mini",
    "Perspective",
    "NoteSkin",
    "NoteSkinVariant",
    "JudgmentGraphic",
    "ComboFont",
    "HoldJudgment",
    "BackgroundFilter",
    "NoteFieldOffsetX",
    "NoteFieldOffsetY",
    "VisualDelay",
    "MusicRate",
    "Stepchart",
    "ScreenAfterPlayerOptions",
    "Turn",
    "Scroll",
    "Hide",
    "LifeMeterType",
    "DataVisualizations",
    "TargetScore",
    "ActionOnMissedTarget",
    "GameplayExtras",
    "GameplayExtrasB",
    "GameplayExtrasC",
    "TiltMultiplier",
    "ErrorBar",
    "ErrorBarTrim",
    "ErrorBarOptions",
    "MeasureCounter",
    "MeasureCounterOptions",
    "MeasureLines",
    "TimingWindowOptions",
    "TimingWindows",
    "FaPlus",
    "ScoreBoxOptions",
    "StepStatsExtra",
    "FunOptions",
    "LifeBarOptions",
    "ComboColors",
    "ComboMode",
    "TimerMode",
    "JudgmentAnimation",
    "RailBalance",
    "ExtraAesthetics",
    "ScreenAfterPlayerOptions2",
    "Insert",
    "Remove",
    "Holds",
    "Attacks",
    "Characters",
    "HideLightType",
    "ScreenAfterPlayerOptions3",
    "Assist",
    "ShowBGChangesPlay",
    "ScreenAfterPlayerOptions4",
];

fn top_screen_option_row_name(value: Option<Value>) -> String {
    match value {
        Some(Value::String(name)) => name
            .to_str()
            .map(|name| name.to_string())
            .unwrap_or_default(),
        Some(value) => read_i32_value(value)
            .and_then(|index| top_screen_option_row_name_at(index).map(str::to_string))
            .unwrap_or_else(|| SONG_LUA_TOP_SCREEN_OPTION_ROWS[0].to_string()),
        None => SONG_LUA_TOP_SCREEN_OPTION_ROWS[0].to_string(),
    }
}

fn top_screen_option_row_name_at(index: i32) -> Option<&'static str> {
    let index = usize::try_from(index).ok()?;
    SONG_LUA_TOP_SCREEN_OPTION_ROWS
        .get(index)
        .or_else(|| {
            index
                .checked_sub(1)
                .and_then(|index| SONG_LUA_TOP_SCREEN_OPTION_ROWS.get(index))
        })
        .copied()
}

fn create_option_row_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let row = create_dummy_actor(lua, "OptionRow")?;
    row.set("Name", name)?;
    set_string_method(lua, &row, "GetName", name)?;
    let frame = create_option_row_frame(lua, name)?;
    frame.set("__songlua_parent", row.clone())?;
    actor_children(lua, &row)?.set("", frame)?;
    row.set(
        "GetChoiceInRowWithFocus",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    Ok(row)
}

fn create_option_row_frame(lua: &Lua, row_name: &str) -> mlua::Result<Table> {
    let frame = create_dummy_actor(lua, "OptionRowFrame")?;
    let title = create_option_row_text_actor(lua, "Title", row_name)?;
    title.set("__songlua_parent", frame.clone())?;
    actor_children(lua, &frame)?.set("Title", title)?;

    let items = lua.create_table()?;
    items.set("__songlua_child_group", true)?;
    let item_text = option_row_default_text(row_name);
    for index in 1..=LUA_PLAYERS {
        let item = create_option_row_text_actor(lua, "Item", &item_text)?;
        item.set("__songlua_parent", frame.clone())?;
        items.raw_set(index, item)?;
    }
    actor_children(lua, &frame)?.set("Item", items)?;
    Ok(frame)
}

fn create_option_row_text_actor(lua: &Lua, name: &str, text: &str) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "BitmapText")?;
    actor.set("Name", name)?;
    actor.set("Text", text)?;
    Ok(actor)
}

fn option_row_default_text(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "speedmodtype" => "X".to_string(),
        "speedmod" => "1".to_string(),
        "mini" => "0%".to_string(),
        "perspective" => "Overhead".to_string(),
        "noteskin" | "noteskinvariant" => "default".to_string(),
        "musicrate" => "1".to_string(),
        _ => custom_option_row_spec(name)
            .map(|spec| option_value_text(spec.choices, 0))
            .unwrap_or_default(),
    }
}

fn option_value_text(values: LuaOptionValues, index: usize) -> String {
    match values {
        LuaOptionValues::Str(values) => values.get(index).copied().unwrap_or_default().to_string(),
        LuaOptionValues::Bool(values) => values.get(index).copied().unwrap_or(false).to_string(),
        LuaOptionValues::Int(values) => values.get(index).copied().unwrap_or_default().to_string(),
        LuaOptionValues::Number(values) => {
            values.get(index).copied().unwrap_or_default().to_string()
        }
    }
}

fn create_top_screen_player_actor(
    lua: &Lua,
    player: SongLuaPlayerContext,
    player_index: usize,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "PlayerActor")?;
    actor.set("Name", top_screen_player_name(player_index))?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_visible", true)?;
    actor.set("__songlua_state_x", player.screen_x)?;
    actor.set("__songlua_state_y", player.screen_y)?;
    Ok(actor)
}

fn top_screen_player_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "PlayerP1",
        1 => "PlayerP2",
        _ => "",
    }
}

fn top_screen_player_index(name: &str) -> Option<usize> {
    match name {
        "PlayerP1" => Some(0),
        "PlayerP2" => Some(1),
        _ => None,
    }
}

fn top_screen_life_meter_index(name: &str) -> Option<usize> {
    match name {
        "LifeP1" => Some(0),
        "LifeP2" => Some(1),
        _ => None,
    }
}

fn top_screen_life_meter_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "LifeP1",
        1 => "LifeP2",
        _ => "",
    }
}

fn top_screen_score_index(name: &str) -> Option<usize> {
    match name {
        "ScoreP1" => Some(0),
        "ScoreP2" => Some(1),
        _ => None,
    }
}

fn top_screen_score_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "ScoreP1",
        1 => "ScoreP2",
        _ => "",
    }
}

fn top_screen_score_percent_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "PercentP1",
        1 => "PercentP2",
        _ => "",
    }
}

fn top_screen_steps_display_index(name: &str) -> Option<usize> {
    match name {
        "StepsDisplayP1" => Some(0),
        "StepsDisplayP2" => Some(1),
        _ => None,
    }
}

fn top_screen_steps_text(parent: &Table, player_index: usize) -> mlua::Result<String> {
    parent
        .get::<Option<String>>(match player_index {
            0 => "__songlua_steps_text_1",
            1 => "__songlua_steps_text_2",
            _ => "",
        })?
        .map_or_else(|| Ok(SongLuaDifficulty::Medium.sm_name().to_string()), Ok)
}

fn top_screen_song_meter_display_index(name: &str) -> Option<usize> {
    match name {
        "SongMeterDisplayP1" => Some(0),
        "SongMeterDisplayP2" => Some(1),
        _ => None,
    }
}

fn top_screen_life_meter_bar_index(name: &str) -> Option<usize> {
    match name {
        "LifeMeterBarP1" => Some(0),
        "LifeMeterBarP2" => Some(1),
        _ => None,
    }
}

fn underlay_score_index(name: &str) -> Option<usize> {
    match name {
        "P1Score" => Some(0),
        "P2Score" => Some(1),
        _ => None,
    }
}

fn underlay_score_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "P1Score",
        1 => "P2Score",
        _ => "",
    }
}

fn top_screen_step_stats_pane_index(name: &str) -> Option<usize> {
    match name {
        "StepStatsPaneP1" => Some(0),
        "StepStatsPaneP2" => Some(1),
        _ => None,
    }
}

fn top_screen_danger_index(name: &str) -> Option<usize> {
    match name {
        "DangerP1" => Some(0),
        "DangerP2" => Some(1),
        _ => None,
    }
}

fn create_player_tables(
    lua: &Lua,
    context: &SongLuaCompileContext,
    song_runtime: &Table,
) -> mlua::Result<PlayerLuaTables> {
    let player_states = [
        create_player_state_table(lua, context.players[0].clone(), 0, song_runtime)?,
        create_player_state_table(lua, context.players[1].clone(), 1, song_runtime)?,
    ];
    let steps = [
        create_steps_table(
            lua,
            context.players[0].difficulty,
            context.players[0].display_bpms,
            context.song_dir.as_path(),
        )?,
        create_steps_table(
            lua,
            context.players[1].difficulty,
            context.players[1].display_bpms,
            context.song_dir.as_path(),
        )?,
    ];
    Ok(PlayerLuaTables {
        player_states,
        steps,
    })
}

fn create_enabled_players_table(
    lua: &Lua,
    players: [SongLuaPlayerContext; LUA_PLAYERS],
) -> mlua::Result<Table> {
    let enabled = lua.create_table()?;
    let mut next_index = 1;
    for (player_index, player) in players.into_iter().enumerate() {
        if !player.enabled {
            continue;
        }
        enabled.set(next_index, player_number_name(player_index))?;
        next_index += 1;
    }
    Ok(enabled)
}

fn create_theme_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let theme = lua.create_table()?;
    let human_player_count = song_lua_human_player_count(context);
    let screen_height = context.screen_height.max(1.0);
    set_string_method(lua, &theme, "GetCurThemeName", SONG_LUA_THEME_NAME)?;
    set_string_method(lua, &theme, "GetThemeDisplayName", SONG_LUA_THEME_NAME)?;
    set_string_method(lua, &theme, "GetCurLanguage", "en")?;
    set_string_method(
        lua,
        &theme,
        "GetCurrentThemeDirectory",
        SONG_LUA_THEME_PATH_PREFIX,
    )?;
    set_string_method(lua, &theme, "GetThemeAuthor", "")?;
    theme.set(
        "GetNumSelectableThemes",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    for method in ["DoesThemeExist", "IsThemeSelectable"] {
        theme.set(
            method,
            lua.create_function(|_, args: MultiValue| {
                let name = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                Ok(name.eq_ignore_ascii_case(SONG_LUA_THEME_NAME))
            })?,
        )?;
    }
    theme.set(
        "DoesLanguageExist",
        lua.create_function(|_, args: MultiValue| {
            let name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(matches!(
                name.to_ascii_lowercase().as_str(),
                "en" | "english"
            ))
        })?,
    )?;
    theme.set(
        "get_theme_fallback_list",
        lua.create_function(|lua, _args: MultiValue| {
            create_string_array(lua, &[SONG_LUA_THEME_NAME])
        })?,
    )?;
    theme.set(
        "RunLuaScripts",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    theme.set(
        "GetMetric",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            theme_metric_value_for_human_players(
                lua,
                &group,
                &name,
                human_player_count,
                screen_height,
            )
        })?,
    )?;
    theme.set(
        "GetMetricF",
        lua.create_function(move |_, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            Ok(
                theme_metric_number_for_screen(&group, &name, human_player_count, screen_height)
                    .map_or(Value::Nil, |value| Value::Number(value as f64)),
            )
        })?,
    )?;
    theme.set(
        "GetMetricI",
        lua.create_function(move |_, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            Ok(
                theme_metric_number_for_screen(&group, &name, human_player_count, screen_height)
                    .map_or(Value::Nil, |value| Value::Integer(value.round() as i64)),
            )
        })?,
    )?;
    theme.set(
        "GetMetricB",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(theme_metric_bool(theme_metric_value_for_human_players(
                lua,
                &group,
                &name,
                human_player_count,
                screen_height,
            )?))
        })?,
    )?;
    theme.set(
        "HasMetric",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(!matches!(
                theme_metric_value_for_human_players(
                    lua,
                    &group,
                    &name,
                    human_player_count,
                    screen_height,
                )?,
                Value::Nil
            ))
        })?,
    )?;
    theme.set(
        "GetMetricNamesInGroup",
        lua.create_function(|lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let names = theme_metric_names(&group);
            if names.is_empty() {
                Ok(Value::Nil)
            } else {
                Ok(Value::Table(create_owned_string_array(lua, &names)?))
            }
        })?,
    )?;
    theme.set(
        "GetStringNamesInGroup",
        lua.create_function(|lua, args: MultiValue| {
            let Some(section) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let names = theme_string_names(&section);
            if names.is_empty() {
                Ok(Value::Nil)
            } else {
                Ok(Value::Table(create_owned_string_array(lua, &names)?))
            }
        })?,
    )?;
    theme.set(
        "GetPathInfoB",
        lua.create_function(|lua, args: MultiValue| {
            let group = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let name = method_arg(&args, 1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok((
                lua.create_string(theme_path("B", &group, &name))?,
                lua.create_string(&group)?,
                lua.create_string(&name)?,
            ))
        })?,
    )?;
    for (method_name, kind) in [
        ("GetPathB", "B"),
        ("GetPathF", "F"),
        ("GetPathG", "G"),
        ("GetPathO", "O"),
        ("GetPathS", "S"),
    ] {
        theme.set(
            method_name,
            lua.create_function({
                let kind = kind.to_string();
                move |lua, args: MultiValue| {
                    let group = method_arg(&args, 0)
                        .cloned()
                        .and_then(read_string)
                        .unwrap_or_default();
                    let name = method_arg(&args, 1)
                        .cloned()
                        .and_then(read_string)
                        .unwrap_or_default();
                    Ok(Value::String(
                        lua.create_string(theme_path(&kind, &group, &name))?,
                    ))
                }
            })?,
        )?;
    }
    theme.set(
        "HasString",
        lua.create_function(|_, args: MultiValue| {
            let Some(section) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(theme_has_string(&section, &name))
        })?,
    )?;
    theme.set(
        "GetString",
        lua.create_function(|lua, args: MultiValue| {
            let Some(section) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::String(lua.create_string("")?));
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::String(lua.create_string("")?));
            };
            Ok(Value::String(
                lua.create_string(theme_string(&section, &name))?,
            ))
        })?,
    )?;
    theme.set(
        "GetSelectableThemeNames",
        lua.create_function(|lua, _args: MultiValue| {
            let names = lua.create_table()?;
            names.raw_set(1, SONG_LUA_THEME_NAME)?;
            Ok(names)
        })?,
    )?;
    for name in ["ReloadMetrics", "SetTheme"] {
        theme.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(theme)
}

fn create_gameman_table(lua: &Lua) -> mlua::Result<Table> {
    let gameman = lua.create_table()?;
    gameman.set(
        "GetStylesForGame",
        lua.create_function(|lua, _args: MultiValue| {
            let styles = lua.create_table()?;
            styles.raw_set(1, create_style_table(lua, "single")?)?;
            Ok(styles)
        })?,
    )?;
    Ok(gameman)
}

fn create_charman_table(lua: &Lua) -> mlua::Result<Table> {
    let charman = lua.create_table()?;
    charman.set(
        "GetAllCharacters",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    charman.set(
        "GetCharacterCount",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    for method in ["GetCharacter", "GetDefaultCharacter", "GetRandomCharacter"] {
        charman.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    Ok(charman)
}

fn song_lua_sound_paths_table(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    if let Some(paths) = globals.get::<Option<Table>>(SONG_LUA_SOUND_PATHS_KEY)? {
        return Ok(paths);
    }
    let paths = lua.create_table()?;
    globals.set(SONG_LUA_SOUND_PATHS_KEY, paths.clone())?;
    Ok(paths)
}

fn record_song_lua_sound_path(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<()> {
    let Some(raw_path) = method_arg(args, 0).cloned().and_then(read_string) else {
        return Ok(());
    };
    let Ok(path) = resolve_script_path(lua, song_dir, &raw_path) else {
        return Ok(());
    };
    if !path.is_file() || !is_song_lua_audio_path(path.as_path()) {
        return Ok(());
    }

    let key = path.to_string_lossy().into_owned();
    let paths = song_lua_sound_paths_table(lua)?;
    for existing in paths.sequence_values::<String>() {
        if existing? == key {
            return Ok(());
        }
    }
    paths.raw_set(paths.raw_len() + 1, key)?;
    Ok(())
}

fn create_sound_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let sound = lua.create_table()?;
    for name in ["DimMusic", "StopMusic"] {
        let sound_for_return = sound.clone();
        sound.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(sound_for_return.clone())
            })?,
        )?;
    }
    for name in ["PlayMusicPart", "PlayOnce"] {
        let sound_for_return = sound.clone();
        sound.set(
            name,
            lua.create_function({
                let song_dir = song_dir.to_path_buf();
                move |lua, args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    record_song_lua_sound_path(lua, song_dir.as_path(), &args)?;
                    Ok(sound_for_return.clone())
                }
            })?,
        )?;
    }
    let sound_for_return = sound.clone();
    sound.set(
        "PlayAnnouncer",
        lua.create_function(move |lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(sound_for_return.clone())
        })?,
    )?;
    sound.set(
        "GetPlayerBalance",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    sound.set(
        "IsTimingDelayed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(sound)
}

fn theme_metric_value_for_human_players(
    lua: &Lua,
    group: &str,
    name: &str,
    human_player_count: usize,
    screen_height: f32,
) -> mlua::Result<Value> {
    if let Some(value) =
        theme_metric_number_for_screen(group, name, human_player_count, screen_height)
    {
        return Ok(Value::Number(value as f64));
    }
    if let Some(value) = theme_metric_string(group, name) {
        return Ok(Value::String(lua.create_string(&value)?));
    }
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("DefaultNoteSkinName") {
        return Ok(Value::String(lua.create_string("default")?));
    }
    if name.eq_ignore_ascii_case("Class") {
        return Ok(Value::String(lua.create_string(group)?));
    }
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("AutoSetStyle")
        || group.eq_ignore_ascii_case("ScreenHeartEntry")
            && name.eq_ignore_ascii_case("HeartEntryEnabled")
    {
        return Ok(Value::Boolean(false));
    }
    Ok(Value::Nil)
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

fn theme_metric_number(group: &str, name: &str) -> Option<f32> {
    theme_metric_number_for_human_players(group, name, LUA_PLAYERS)
}

fn theme_metric_number_for_human_players(
    group: &str,
    name: &str,
    human_player_count: usize,
) -> Option<f32> {
    theme_metric_number_for_screen(group, name, human_player_count, 480.0)
}

fn theme_metric_number_for_screen(
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

fn graph_display_body_size(human_player_count: usize) -> [f32; 2] {
    [
        if human_player_count == 1 {
            610.0
        } else {
            300.0
        },
        64.0,
    ]
}

fn song_lua_human_player_count(context: &SongLuaCompileContext) -> usize {
    context
        .players
        .iter()
        .filter(|player| player.enabled)
        .count()
}

fn theme_metric_bool(value: Value) -> bool {
    match value {
        Value::Boolean(value) => value,
        Value::Integer(value) => value != 0,
        Value::Number(value) => value != 0.0,
        Value::String(value) => !value.to_str().is_ok_and(|text| text.is_empty()),
        _ => false,
    }
}

fn theme_metric_names(group: &str) -> Vec<String> {
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

fn theme_string_names(section: &str) -> Vec<String> {
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

fn theme_path(kind: &str, group: &str, name: &str) -> String {
    let group = group.trim_matches('/');
    let name = name.trim_start_matches('/');
    if group.is_empty() {
        format!("{SONG_LUA_THEME_PATH_PREFIX}{kind}/{name}")
    } else {
        format!("{SONG_LUA_THEME_PATH_PREFIX}{kind}/{group}/{name}")
    }
}

#[derive(Clone, Copy)]
enum LuaOptionValues {
    Str(&'static [&'static str]),
    Bool(&'static [bool]),
    Int(&'static [i64]),
    Number(&'static [f64]),
}

impl LuaOptionValues {
    fn len(self) -> usize {
        match self {
            Self::Str(values) => values.len(),
            Self::Bool(values) => values.len(),
            Self::Int(values) => values.len(),
            Self::Number(values) => values.len(),
        }
    }
}

#[derive(Clone, Copy)]
struct SongLuaOptionRowSpec {
    choices: LuaOptionValues,
    values: Option<LuaOptionValues>,
    layout_type: &'static str,
    select_type: &'static str,
    one_choice_for_all_players: bool,
    export_on_change: bool,
    hide_on_disable: bool,
    reload_row_messages: &'static [&'static str],
    broadcast_on_export: &'static [&'static str],
}

impl SongLuaOptionRowSpec {
    fn new(choices: LuaOptionValues) -> Self {
        Self {
            choices,
            values: None,
            layout_type: "ShowAllInRow",
            select_type: "SelectOne",
            one_choice_for_all_players: false,
            export_on_change: false,
            hide_on_disable: false,
            reload_row_messages: &[],
            broadcast_on_export: &[],
        }
    }

    fn values(mut self, values: LuaOptionValues) -> Self {
        self.values = Some(values);
        self
    }

    fn layout(mut self, layout_type: &'static str) -> Self {
        self.layout_type = layout_type;
        self
    }

    fn select(mut self, select_type: &'static str) -> Self {
        self.select_type = select_type;
        self
    }

    fn one_choice(mut self) -> Self {
        self.one_choice_for_all_players = true;
        self
    }

    fn export(mut self) -> Self {
        self.export_on_change = true;
        self
    }

    fn hide_on_disable(mut self) -> Self {
        self.hide_on_disable = true;
        self
    }

    fn reload(mut self, messages: &'static [&'static str]) -> Self {
        self.reload_row_messages = messages;
        self
    }
}

const OPTION_YES_NO: &[&str] = &["Yes", "No"];
const OPTION_ON_OFF: &[&str] = &["On", "Off"];
const OPTION_OFF_ON: &[&str] = &["Off", "On"];
const OPTION_TRUE_FALSE: &[bool] = &[true, false];
const OPTION_FALSE_TRUE: &[bool] = &[false, true];
const OPTION_EMPTY: &[&str] = &[""];
const OPTION_NONE: &[&str] = &["None"];
const OPTION_ONE_TO_TWELVE: &[i64] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
const OPTION_ZERO_TO_NINE: &[i64] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
const OPTION_CASUAL_METERS: &[i64] = &[5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
const OPTION_MENU_TIMER_CHOICES: &[&str] = &[
    "0:15", "0:30", "1:00", "1:30", "2:00", "3:00", "5:00", "7:30",
];
const OPTION_MENU_TIMER_VALUES: &[i64] = &[15, 30, 60, 90, 120, 180, 300, 450];
const OPTION_NICE_CHOICES: &[&str] = &["Off", "On", "OnWithSound"];
const OPTION_NICE_VALUES: &[i64] = &[0, 1, 2];
const OPTION_VISUAL_STYLE: &[&str] = &[
    "Hearts",
    "Arrows",
    "Bears",
    "Ducks",
    "Cats",
    "Spooky",
    "Gay",
    "Stars",
    "Thonk",
    "Technique",
    "SRPG9",
];
const OPTION_GAME_MODE: &[&str] = &["Casual", "ITG"];
const OPTION_AUTO_STYLE: &[&str] = &["none", "single", "versus", "double"];
const OPTION_MUSIC_WHEEL_STYLE: &[&str] = &["ITG", "IIDX"];
const OPTION_THEME_FONT: &[&str] = &["Common"];
const OPTION_BG_STYLE: &[&str] = &["Off", "Random"];
const OPTION_QR_LOGIN: &[&str] = &["Always", "Sometimes", "Never"];
const OPTION_SCORING_SYSTEM: &[&str] = &["EX", "ITG"];
const OPTION_STEP_STATS: &[&str] = &["Show", "Hide"];
const OPTION_SPEED_MOD_TYPE: &[&str] = &["X", "C", "M"];
const OPTION_SPEED_MOD: &[&str] = &["1", "1.5", "2", "C400", "M650"];
const OPTION_MINI: &[&str] = &["-100%", "0%", "25%", "50%", "100%", "150%"];
const OPTION_SPACING: &[&str] = &["-100%", "0%", "25%", "50%", "100%"];
const OPTION_NOTESKIN: &[&str] = &["default"];
const OPTION_BACKGROUND_FILTER: &[&str] = &["Off", "Dark", "Darker", "Darkest"];
const OPTION_NOTE_FIELD_OFFSET: &[&str] = &["0", "10", "25", "50"];
const OPTION_VISUAL_DELAY: &[&str] = &["-100ms", "0ms", "100ms"];
const OPTION_MUSIC_RATE: &[&str] = &["0.75", "1", "1.25", "1.5", "2"];
const OPTION_STEPCHART: &[&str] = &["Easy 1", "Medium 5", "Hard 9"];
const OPTION_SCREEN_AFTER_PLAYER_OPTIONS: &[&str] = &[
    "ScreenGameplay",
    "ScreenSelectMusic",
    "ScreenPlayerOptions",
    "ScreenPlayerOptions2",
];
const OPTION_HIDE: &[&str] = &[
    "Targets",
    "SongBG",
    "Combo",
    "Lifebar",
    "Score",
    "Danger",
    "ComboExplosions",
];
const OPTION_GAMEPLAY_EXTRAS: &[&str] = &[
    "ColumnFlashOnMiss",
    "SubtractiveScoring",
    "Pacemaker",
    "NPSGraphAtTop",
    "JudgmentTilt",
    "ColumnCues",
];
const OPTION_RESULTS_EXTRAS: &[&str] = &["TargetScore", "EvaluationPane", "Graphs"];
const OPTION_LIFE_METER_TYPE: &[&str] = &["Standard", "Battery"];
const OPTION_DATA_VISUALIZATIONS: &[&str] = &["None", "Target Score Graph", "Step Statistics"];
const OPTION_TARGET_SCORE: &[&str] = &[
    "GradeTier16",
    "GradeTier10",
    "Machine best",
    "Personal best",
];
const OPTION_TARGET_SCORE_NUMBER: &[&str] = &["1", "2", "3", "4", "5"];
const OPTION_ACTION_ON_MISSED_TARGET: &[&str] = &["Nothing", "Fail", "Restart"];
const OPTION_TILT_MULTIPLIER: &[&str] = &["1", "1.5", "2", "2.5", "3"];
const OPTION_ERROR_BAR: &[&str] = &["None", "Colorful", "Monochrome", "Text"];
const OPTION_ERROR_BAR_TRIM: &[&str] = &["Off", "Great", "Excellent"];
const OPTION_ERROR_BAR_OPTIONS: &[&str] = &["ErrorBarUp", "ErrorBarMultiTick"];
const OPTION_MEASURE_COUNTER: &[&str] = &["None", "8th", "12th", "16th", "24th", "32nd"];
const OPTION_MEASURE_COUNTER_OPTIONS: &[&str] =
    &["MeasureCounterLeft", "MeasureCounterUp", "HideLookahead"];
const OPTION_MEASURE_COUNTER_LOOKAHEAD: &[&str] = &["0", "1", "2", "4"];
const OPTION_MEASURE_LINES: &[&str] = &["Off", "Measure", "Quarter", "Eighth"];
const OPTION_TIMING_WINDOW_OPTIONS: &[&str] = &[
    "HideEarlyDecentWayOffJudgments",
    "HideEarlyDecentWayOffFlash",
];
const OPTION_TIMING_WINDOWS: &[&str] = &["All", "Hide Way Off", "Hide Decents and Way Offs"];
const OPTION_FA_PLUS: &[&str] = &["ShowFaPlusWindow", "ShowExScore", "ShowFaPlusPane"];
const OPTION_LIFE_BAR_OPTIONS: &[&str] = &["Normal", "Vertical", "Hidden"];
const OPTION_SCORE_BOX_OPTIONS: &[&str] = &["Machine", "Personal", "Rival"];
const OPTION_STEP_STATS_EXTRA: &[&str] = &["DensityGraph", "Measures", "Streams"];
const OPTION_FUN_OPTIONS: &[&str] = &["Confetti", "LaneCover", "ScreenFilter"];
const OPTION_COMBO_COLORS: &[&str] = &["Default", "Difficulty", "Judgment"];
const OPTION_COMBO_MODE: &[&str] = &["Standard", "Additive", "Proportional"];
const OPTION_TIMER_MODE: &[&str] = &["Song", "Remaining", "Off"];
const OPTION_JUDGMENT_ANIMATION: &[&str] = &["Default", "ProITG", "None"];
const OPTION_RAIL_BALANCE: &[&str] = &["Off", "Standard", "Strict"];
const OPTION_EXTRA_AESTHETICS: &[&str] = &["Backgrounds", "Particles", "ScreenFX"];
const OPTION_THEME_NAMES: &[&str] = &[SONG_LUA_THEME_NAME];
const OPTION_FAIL_TYPES: &[&str] = &["Immediate", "ImmediateContinue", "Off"];
const OPTION_LONG_TIME: &[&str] = &["2:30", "3:00", "4:00", "5:00", "Off"];
const OPTION_MARATHON_TIME: &[&str] = &["5:00", "7:30", "10:00", "15:00", "Off"];
const OPTION_LONG_TIME_VALUES: &[i64] = &[150, 180, 240, 300, 999_999];
const OPTION_MARATHON_TIME_VALUES: &[i64] = &[300, 450, 600, 900, 999_999];
const OPTION_MUSIC_WHEEL_SPEED: &[&str] = &[
    "Slow",
    "Normal",
    "Fast",
    "Faster",
    "Ridiculous",
    "Ludicrous",
    "Plaid",
];
const OPTION_MUSIC_WHEEL_SPEED_VALUES: &[i64] = &[5, 10, 15, 25, 30, 45, 100];
const OPTION_VIDEO_RENDERER: &[&str] = &["opengl"];
const OPTION_DISPLAY_ASPECT_RATIO: &[&str] = &["16:9", "4:3"];
const OPTION_DISPLAY_ASPECT_RATIO_VALUES: &[f64] = &[16.0 / 9.0, 4.0 / 3.0];
const OPTION_DISPLAY_RESOLUTION: &[&str] = &["1920x1080", "1280x720", "640x480"];
const OPTION_DISPLAY_MODE: &[&str] = &["Windowed", "Fullscreen"];
const OPTION_REFRESH_RATE: &[&str] = &["60", "120", "144"];
const OPTION_REFRESH_RATE_VALUES: &[i64] = &[60, 120, 144];
const OPTION_FULLSCREEN_TYPE: &[&str] = &["Borderless", "Exclusive"];
const OPTION_OFFSET_MS: &[&str] = &["-1000ms", "-500ms", "0ms", "500ms", "1000ms"];
const OPTION_OFFSET_SECONDS_VALUES: &[f64] = &[-1.0, -0.5, 0.0, 0.5, 1.0];
const OPTION_CUSTOM_SONG_SECONDS: &[&str] = &["1:45", "3:00", "5:00", "10:00", "15:00", "2:00:00"];
const OPTION_CUSTOM_SONG_SECONDS_VALUES: &[i64] = &[105, 180, 300, 600, 900, 7200];
const OPTION_CUSTOM_SONG_MEGABYTES: &[&str] = &["3 MB", "5 MB", "10 MB", "20 MB", "30 MB", "1 GB"];
const OPTION_CUSTOM_SONG_MEGABYTES_VALUES: &[i64] = &[3, 5, 10, 20, 30, 1000];
const OPTION_CUSTOM_SONG_TIMEOUT: &[&str] = &["3", "5", "10", "60"];
const OPTION_CUSTOM_SONG_TIMEOUT_VALUES: &[i64] = &[3, 5, 10, 60];
const OPTION_REFRESH_ACTOR_PROXY_MESSAGES: &[&str] = &["RefreshActorProxy"];
const THEME_PREF_ROW_NAMES: &[&str] = &[
    "AllowFailingOutOfSet",
    "NumberOfContinuesAllowed",
    "HideStockNoteSkins",
    "MusicWheelStyle",
    "AllowDanceSolo",
    "DefaultGameMode",
    "AutoStyle",
    "VisualStyle",
    "AllowThemeVideos",
    "RainbowMode",
    "WriteCustomScores",
    "KeyboardFeatures",
    "SampleMusicLoops",
    "RescoreEarlyHits",
    "AnimateBanners",
    "SimplyLoveColor",
    "EditModeLastSeenSong",
    "EditModeLastSeenStepsType",
    "EditModeLastSeenStyleType",
    "EditModeLastSeenDifficulty",
    "ScreenGrooveStatsLoginMenuTimer",
    "ScreenSelectMusicMenuTimer",
    "ScreenSelectMusicCasualMenuTimer",
    "ScreenPlayerOptionsMenuTimer",
    "ScreenEvaluationMenuTimer",
    "ScreenEvaluationNonstopMenuTimer",
    "ScreenEvaluationSummaryMenuTimer",
    "ScreenNameEntryMenuTimer",
    "AllowScreenSelectProfile",
    "AllowScreenSelectColor",
    "AllowScreenSelectPlayMode",
    "AllowScreenSelectPlayMode2",
    "AllowScreenEvalSummary",
    "AllowScreenGameOver",
    "AllowScreenNameEntry",
    "CasualMaxMeter",
    "UseImageCache",
    "nice",
    "LastActiveEvent",
    "EnableTournamentMode",
    "ScoringSystem",
    "StepStats",
    "EnforceNoCmod",
    "EnableGrooveStats",
    "AutoDownloadUnlocks",
    "SeparateUnlocksByPlayer",
    "QRLogin",
    "EnableOnlineLobbies",
];

fn create_theme_prefs_table(lua: &Lua) -> mlua::Result<Table> {
    let prefs = lua.create_table()?;
    let store = lua.create_table()?;
    let get_store = store.clone();
    prefs.set(
        "Get",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let stored = get_store.get::<Value>(name.as_str())?;
            if !matches!(stored, Value::Nil) {
                return Ok(stored);
            }
            theme_pref_default(lua, &name)
        })?,
    )?;
    let set_store = store.clone();
    prefs.set(
        "Set",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(());
            };
            let value = method_arg(&args, 1).cloned().unwrap_or(Value::Nil);
            set_store.set(name, value)?;
            Ok(())
        })?,
    )?;
    prefs.set(
        "Save",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    let init_store = store;
    prefs.set(
        "InitAll",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let defs = if args.len() == 1 {
                args.front()
            } else {
                method_arg(&args, 0)
            };
            let Some(Value::Table(defs)) = defs else {
                return Ok(());
            };
            for pair in defs.pairs::<String, Table>() {
                let (name, def) = pair?;
                if matches!(init_store.get::<Value>(name.as_str())?, Value::Nil) {
                    let default = def.get::<Value>("Default").unwrap_or(Value::Nil);
                    if !matches!(default, Value::Nil) {
                        init_store.set(name, default)?;
                    }
                }
            }
            Ok(())
        })?,
    )?;
    Ok(prefs)
}

fn create_lua_option_array(lua: &Lua, values: LuaOptionValues) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    match values {
        LuaOptionValues::Str(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        LuaOptionValues::Bool(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        LuaOptionValues::Int(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        LuaOptionValues::Number(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
    }
    Ok(table)
}

fn option_row_list_arg(lua: &Lua, args: &MultiValue) -> mlua::Result<Table> {
    if let Some(Value::Table(table)) = method_arg(args, 0) {
        return Ok(table.clone());
    }
    if let Some(Value::Table(table)) = args.front() {
        return Ok(table.clone());
    }
    lua.create_table()
}

fn option_row_player_arg(args: &MultiValue) -> Option<&Value> {
    if matches!(args.front(), Some(Value::Table(_))) {
        if matches!(args.get(1), Some(Value::Table(_))) {
            args.get(2)
        } else {
            args.get(1)
        }
    } else {
        args.get(1)
    }
}

fn option_row_has_selection(table: &Table, count: usize) -> mlua::Result<bool> {
    for index in 1..=count.max(table.raw_len()) {
        match table.raw_get::<Value>(index)? {
            Value::Boolean(true) => return Ok(true),
            Value::Nil | Value::Boolean(false) => {}
            _ => return Ok(true),
        }
    }
    Ok(false)
}

fn option_row_values_table(row: &Table) -> mlua::Result<Table> {
    match row.get::<Value>("Values")? {
        Value::Table(values) => Ok(values),
        _ => row.get::<Table>("Choices"),
    }
}

fn option_row_selected_value(row: &Table, selections: &Table) -> mlua::Result<Value> {
    let values = option_row_values_table(row)?;
    let count = values.raw_len().max(selections.raw_len());
    for index in 1..=count {
        if truthy(&selections.raw_get::<Value>(index)?) {
            return values.raw_get(index);
        }
    }
    values.raw_get(1)
}

fn set_pref_option_save(lua: &Lua, row: &Table, pref_name: &str) -> mlua::Result<()> {
    let row_for_save = row.clone();
    let pref_name = pref_name.to_string();
    row.set(
        "SaveSelections",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let selections = option_row_list_arg(lua, &args)?;
            let value = option_row_selected_value(&row_for_save, &selections)?;
            let prefsmgr = lua.globals().get::<Table>("PREFSMAN")?;
            let set_preference = prefsmgr.get::<Function>("SetPreference")?;
            let _: Value = set_preference.call((prefsmgr, pref_name.as_str(), value))?;
            Ok(())
        })?,
    )
}

fn set_theme_pref_option_save(lua: &Lua, row: &Table, pref_name: &str) -> mlua::Result<()> {
    let row_for_save = row.clone();
    let pref_name = pref_name.to_string();
    row.set(
        "SaveSelections",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let selections = option_row_list_arg(lua, &args)?;
            let value = option_row_selected_value(&row_for_save, &selections)?;
            let theme_prefs = lua.globals().get::<Table>("ThemePrefs")?;
            let set = theme_prefs.get::<Function>("Set")?;
            set.call::<()>((theme_prefs, pref_name.as_str(), value))
        })?,
    )
}

fn set_custom_option_save(lua: &Lua, row: &Table, option_name: &str) -> mlua::Result<()> {
    let row_for_save = row.clone();
    let option_name = option_name.to_string();
    row.set(
        "SaveSelections",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let selections = option_row_list_arg(lua, &args)?;
            let selected_player = option_row_player_arg(&args)
                .and_then(player_index_from_value)
                .unwrap_or(0);
            let sl = lua.globals().get::<Table>("SL")?;
            if option_name.eq_ignore_ascii_case("MusicRate") {
                let global = sl.get::<Table>("Global")?;
                let mods = global.get::<Table>("ActiveModifiers")?;
                mods.set(
                    "MusicRate",
                    option_row_selected_value(&row_for_save, &selections)?,
                )?;
                return Ok(());
            }
            let player = sl.get::<Table>(player_short_name(selected_player))?;
            let mods = player.get::<Table>("ActiveModifiers")?;
            if row_for_save.get::<String>("SelectType")? == "SelectMultiple" {
                let choices = row_for_save.get::<Table>("Choices")?;
                for index in 1..=choices.raw_len().max(selections.raw_len()) {
                    let selected = truthy(&selections.raw_get::<Value>(index)?);
                    let choice = choices
                        .raw_get::<Value>(index)
                        .ok()
                        .and_then(read_string)
                        .unwrap_or_default();
                    let key = custom_multi_modifier_key(&option_name, &choice);
                    if !key.is_empty() {
                        mods.set(key, selected)?;
                    }
                }
            } else {
                mods.set(
                    option_name.as_str(),
                    option_row_selected_value(&row_for_save, &selections)?,
                )?;
            }
            Ok(())
        })?,
    )
}

fn custom_multi_modifier_key(option_name: &str, choice: &str) -> String {
    if option_name.eq_ignore_ascii_case("Hide") {
        format!("Hide{choice}")
    } else {
        choice.to_string()
    }
}

fn create_compat_option_row_table(
    lua: &Lua,
    name: &str,
    spec: SongLuaOptionRowSpec,
) -> mlua::Result<Table> {
    let row = lua.create_table()?;
    let choice_count = spec.choices.len();
    row.set("Name", name)?;
    row.set("Choices", create_lua_option_array(lua, spec.choices)?)?;
    if let Some(values) = spec.values {
        row.set("Values", create_lua_option_array(lua, values)?)?;
    }
    row.set("LayoutType", spec.layout_type)?;
    row.set("SelectType", spec.select_type)?;
    row.set("OneChoiceForAllPlayers", spec.one_choice_for_all_players)?;
    row.set("ExportOnChange", spec.export_on_change)?;
    row.set("HideOnDisable", spec.hide_on_disable)?;
    row.set(
        "ReloadRowMessages",
        create_string_array(lua, spec.reload_row_messages)?,
    )?;
    row.set(
        "BroadcastOnExport",
        create_string_array(lua, spec.broadcast_on_export)?,
    )?;
    row.set(
        "EnabledForPlayers",
        lua.create_function(|lua, _args: MultiValue| {
            create_string_array(lua, &[player_number_name(0), player_number_name(1)])
        })?,
    )?;
    row.set(
        "LoadSelections",
        lua.create_function(move |lua, args: MultiValue| {
            let list = option_row_list_arg(lua, &args)?;
            if choice_count > 0 && !option_row_has_selection(&list, choice_count)? {
                list.raw_set(1, true)?;
            }
            Ok(list)
        })?,
    )?;
    row.set(
        "SaveSelections",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(row)
}

fn create_custom_option_row(lua: &Lua, name: &str) -> mlua::Result<Option<Table>> {
    let Some(spec) = custom_option_row_spec(name) else {
        return Ok(None);
    };
    let row = create_compat_option_row_table(lua, name, spec)?;
    set_custom_option_save(lua, &row, name)?;
    Ok(Some(row))
}

fn create_conf_option_row(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let (row_name, spec) = match name.to_ascii_lowercase().as_str() {
        "confaspectratio" => (
            "DisplayAspectRatio",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_DISPLAY_ASPECT_RATIO))
                .values(LuaOptionValues::Number(OPTION_DISPLAY_ASPECT_RATIO_VALUES))
                .one_choice(),
        ),
        "confdisplayresolution" => (
            "DisplayResolution",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_DISPLAY_RESOLUTION)).one_choice(),
        ),
        "confdisplaymode" => (
            "DisplayMode",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_DISPLAY_MODE)).one_choice(),
        ),
        "confrefreshrate" => (
            "RefreshRate",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_REFRESH_RATE))
                .values(LuaOptionValues::Int(OPTION_REFRESH_RATE_VALUES))
                .one_choice(),
        ),
        "conffullscreentype" => (
            "FullscreenType",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_FULLSCREEN_TYPE)).one_choice(),
        ),
        _ => (
            name,
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON)).one_choice(),
        ),
    };
    let row = create_compat_option_row_table(lua, row_name, spec)?;
    set_pref_option_save(lua, &row, row_name)?;
    Ok(row)
}

fn custom_option_row_spec(name: &str) -> Option<SongLuaOptionRowSpec> {
    let lower = name.to_ascii_lowercase();
    let spec = match lower.as_str() {
        "speedmodtype" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SPEED_MOD_TYPE))
            .layout("ShowOneInRow")
            .export(),
        "speedmod" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SPEED_MOD))
            .layout("ShowOneInRow")
            .export(),
        "mini" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MINI)).layout("ShowOneInRow")
        }
        "spacing" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SPACING)).layout("ShowOneInRow")
        }
        "noteskin" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NOTESKIN))
            .layout("ShowOneInRow")
            .export(),
        "judgmentgraphic" | "holdjudgment" | "heldgraphic" | "heldmissgraphic" | "combofont" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NONE))
                .layout("ShowOneInRow")
                .export()
        }
        "noteskinvariant" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NONE))
            .layout("ShowOneInRow")
            .export()
            .hide_on_disable()
            .reload(OPTION_REFRESH_ACTOR_PROXY_MESSAGES),
        "backgroundfilter" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_BACKGROUND_FILTER))
                .values(LuaOptionValues::Str(OPTION_BACKGROUND_FILTER))
        }
        "notefieldoffsetx" | "notefieldoffsety" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NOTE_FIELD_OFFSET))
                .layout("ShowOneInRow")
                .export()
        }
        "visualdelay" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_VISUAL_DELAY))
            .layout("ShowOneInRow")
            .export(),
        "musicrate" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MUSIC_RATE))
            .layout("ShowOneInRow")
            .one_choice()
            .export(),
        "stepchart" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_STEPCHART)).export(),
        "screenafterplayeroptions"
        | "screenafterplayeroptions2"
        | "screenafterplayeroptions3"
        | "screenafterplayeroptions4" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SCREEN_AFTER_PLAYER_OPTIONS))
        }
        "hide" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_HIDE)).select("SelectMultiple")
        }
        "gameplayextras" | "gameplayextrasb" | "gameplayextrasc" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_GAMEPLAY_EXTRAS))
                .select("SelectMultiple")
        }
        "resultsextras" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_RESULTS_EXTRAS))
            .select("SelectMultiple"),
        "lifemetertype" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_LIFE_METER_TYPE)),
        "datavisualizations" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_DATA_VISUALIZATIONS))
        }
        "targetscore" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TARGET_SCORE)),
        "targetscorenumber" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TARGET_SCORE_NUMBER))
        }
        "actiononmissedtarget" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ACTION_ON_MISSED_TARGET))
        }
        "tiltmultiplier" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TILT_MULTIPLIER)),
        "errorbar" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ERROR_BAR)),
        "errorbartrim" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ERROR_BAR_TRIM)),
        "errorbaroptions" | "errorbarcap" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ERROR_BAR_OPTIONS))
                .select("SelectMultiple")
        }
        "measurecounter" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MEASURE_COUNTER)),
        "measurecounteroptions" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MEASURE_COUNTER_OPTIONS))
                .select("SelectMultiple")
        }
        "measurecounterlookahead" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MEASURE_COUNTER_LOOKAHEAD))
        }
        "measurelines" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MEASURE_LINES)),
        "timingwindowoptions" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TIMING_WINDOW_OPTIONS))
                .select("SelectMultiple")
        }
        "timingwindows" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TIMING_WINDOWS)),
        "faplus" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_FA_PLUS)).select("SelectMultiple")
        }
        "minindicator" | "miniindicator" | "miniindicatorcolor" | "stepstatsinfo"
        | "judgmentflash" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON)),
        "scoreboxoptions" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SCORE_BOX_OPTIONS))
                .select("SelectMultiple")
        }
        "stepstatsextra" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_STEP_STATS_EXTRA))
                .select("SelectMultiple")
        }
        "funoptions" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_FUN_OPTIONS))
            .select("SelectMultiple"),
        "lifebaroptions" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_LIFE_BAR_OPTIONS))
        }
        "combocolors" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_COMBO_COLORS)),
        "combomode" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_COMBO_MODE)),
        "timermode" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TIMER_MODE)),
        "judgmentanimation" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_JUDGMENT_ANIMATION))
        }
        "railbalance" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_RAIL_BALANCE)),
        "extraaesthetics" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_EXTRA_AESTHETICS))
                .select("SelectMultiple")
        }
        _ => return None,
    };
    Some(spec)
}

fn create_theme_prefs_rows_table(lua: &Lua) -> mlua::Result<Table> {
    let rows = lua.create_table()?;
    rows.set(
        "GetRow",
        lua.create_function(|lua, args: MultiValue| {
            let name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let row = create_compat_option_row_table(lua, &name, theme_pref_row_spec(&name))?;
            set_theme_pref_option_save(lua, &row, &name)?;
            Ok(Value::Table(row))
        })?,
    )?;
    rows.set(
        "InitAll",
        lua.create_function(|lua, args: MultiValue| {
            let defs_arg = if args.len() == 1 {
                args.front()
            } else {
                method_arg(&args, 0)
            };
            let defs = defs_arg
                .cloned()
                .filter(|value| !matches!(value, Value::Nil))
                .unwrap_or(Value::Table(create_theme_pref_defs(lua)?));
            let theme_prefs = lua.globals().get::<Table>("ThemePrefs")?;
            let init = theme_prefs.get::<Function>("InitAll")?;
            init.call::<()>((defs,))
        })?,
    )?;
    Ok(rows)
}

fn create_sl_custom_prefs_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "Get",
        lua.create_function(|lua, _args: MultiValue| create_theme_pref_defs(lua))?,
    )?;
    table.set(
        "Validate",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    table.set(
        "Init",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
}

fn create_theme_pref_defs(lua: &Lua) -> mlua::Result<Table> {
    let defs = lua.create_table()?;
    for name in THEME_PREF_ROW_NAMES {
        defs.set(*name, create_theme_pref_def(lua, name)?)?;
    }
    Ok(defs)
}

fn create_theme_pref_def(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let spec = theme_pref_row_spec(name);
    let def = lua.create_table()?;
    def.set("Default", theme_pref_default(lua, name)?)?;
    def.set("Choices", create_lua_option_array(lua, spec.choices)?)?;
    if let Some(values) = spec.values {
        def.set("Values", create_lua_option_array(lua, values)?)?;
    }
    Ok(def)
}

fn theme_pref_row_spec(name: &str) -> SongLuaOptionRowSpec {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "numberofcontinuesallowed" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Int(OPTION_ZERO_TO_NINE))
                .values(LuaOptionValues::Int(OPTION_ZERO_TO_NINE))
                .one_choice()
        }
        "casualmaxmeter" => SongLuaOptionRowSpec::new(LuaOptionValues::Int(OPTION_CASUAL_METERS))
            .values(LuaOptionValues::Int(OPTION_CASUAL_METERS))
            .one_choice(),
        "simplylovecolor" => SongLuaOptionRowSpec::new(LuaOptionValues::Int(OPTION_ONE_TO_TWELVE))
            .values(LuaOptionValues::Int(OPTION_ONE_TO_TWELVE))
            .one_choice(),
        "nice" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NICE_CHOICES))
            .values(LuaOptionValues::Int(OPTION_NICE_VALUES))
            .one_choice(),
        "screengroovestatsloginmenutimer"
        | "screenselectmusicmenutimer"
        | "screenselectmusiccasualmenutimer"
        | "screenplayeroptionsmenutimer"
        | "screenevaluationmenutimer"
        | "screenevaluationnonstopmenutimer"
        | "screenevaluationsummarymenutimer"
        | "screennameentrymenutimer" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MENU_TIMER_CHOICES))
                .values(LuaOptionValues::Int(OPTION_MENU_TIMER_VALUES))
                .one_choice()
        }
        "visualstyle" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_VISUAL_STYLE))
            .values(LuaOptionValues::Str(OPTION_VISUAL_STYLE))
            .one_choice(),
        "defaultgamemode" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_GAME_MODE))
            .values(LuaOptionValues::Str(OPTION_GAME_MODE))
            .one_choice(),
        "autostyle" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_AUTO_STYLE))
            .values(LuaOptionValues::Str(OPTION_AUTO_STYLE))
            .one_choice(),
        "musicwheelstyle" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MUSIC_WHEEL_STYLE))
                .values(LuaOptionValues::Str(OPTION_MUSIC_WHEEL_STYLE))
                .one_choice()
        }
        "themefont" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_THEME_FONT))
            .values(LuaOptionValues::Str(OPTION_THEME_FONT))
            .one_choice(),
        "songselectbg" | "resultsbg" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_BG_STYLE))
                .values(LuaOptionValues::Str(OPTION_BG_STYLE))
                .one_choice()
        }
        "qrlogin" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_QR_LOGIN))
            .values(LuaOptionValues::Str(OPTION_QR_LOGIN))
            .one_choice(),
        "scoringsystem" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SCORING_SYSTEM))
            .values(LuaOptionValues::Str(OPTION_SCORING_SYSTEM))
            .one_choice(),
        "stepstats" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_STEP_STATS))
            .values(LuaOptionValues::Str(OPTION_STEP_STATS))
            .one_choice(),
        "editmodelastseensong"
        | "editmodelastseendifficulty"
        | "editmodelastseenstepstype"
        | "editmodelastseenstyletype"
        | "lastactiveevent" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_EMPTY))
            .values(LuaOptionValues::Str(OPTION_EMPTY))
            .one_choice(),
        "rainbowmode" | "animatebanners" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ON_OFF))
                .values(LuaOptionValues::Bool(OPTION_TRUE_FALSE))
                .one_choice()
        }
        "hidestocknoteskins" | "memorycards" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON))
                .values(LuaOptionValues::Bool(OPTION_FALSE_TRUE))
                .one_choice()
        }
        _ => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_YES_NO))
            .values(LuaOptionValues::Bool(OPTION_TRUE_FALSE))
            .one_choice(),
    }
}

fn create_operator_menu_option_rows_table(lua: &Lua) -> mlua::Result<Table> {
    let rows = lua.create_table()?;
    for method_name in [
        "Theme",
        "EditorNoteskin",
        "DefaultFailType",
        "LongAndMarathonTime",
        "MusicWheelSpeed",
        "VideoRenderer",
        "GlobalOffsetSeconds",
        "VisualDelaySeconds",
        "MemoryCards",
        "CustomSongsMaxSeconds",
        "CustomSongsMaxMegabytes",
        "CustomSongsLoadTimeout",
    ] {
        rows.set(
            method_name,
            lua.create_function({
                let method_name = method_name.to_string();
                move |lua, args: MultiValue| {
                    create_operator_menu_option_row(lua, &method_name, &args).map(Value::Table)
                }
            })?,
        )?;
    }

    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(|lua, args: MultiValue| {
            let method_name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::Function(lua.create_function(
                move |lua, args: MultiValue| {
                    create_operator_menu_option_row(lua, &method_name, &args).map(Value::Table)
                },
            )?))
        })?,
    )?;
    let _ = rows.set_metatable(Some(mt));
    Ok(rows)
}

fn create_operator_menu_option_row(
    lua: &Lua,
    method_name: &str,
    args: &MultiValue,
) -> mlua::Result<Table> {
    let lower = method_name.to_ascii_lowercase();
    let (row_name, spec, pref_name) = match lower.as_str() {
        "theme" => (
            "Theme".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_THEME_NAMES)).one_choice(),
            Some("Theme".to_string()),
        ),
        "editornoteskin" => (
            "EditorNoteSkin".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NOTESKIN))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("EditorNoteSkinP1".to_string()),
        ),
        "defaultfailtype" => (
            "DefaultFailType".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_FAIL_TYPES)).one_choice(),
            Some("DefaultModifiers".to_string()),
        ),
        "longandmarathontime" => {
            let kind = method_arg(args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_else(|| "Long".to_string());
            let (choices, values, pref_name) = if kind.eq_ignore_ascii_case("Marathon") {
                (
                    OPTION_MARATHON_TIME,
                    OPTION_MARATHON_TIME_VALUES,
                    "MarathonVerSongSeconds",
                )
            } else {
                (
                    OPTION_LONG_TIME,
                    OPTION_LONG_TIME_VALUES,
                    "LongVerSongSeconds",
                )
            };
            (
                format!("{kind} Time"),
                SongLuaOptionRowSpec::new(LuaOptionValues::Str(choices))
                    .values(LuaOptionValues::Int(values))
                    .layout("ShowOneInRow")
                    .one_choice(),
                Some(pref_name.to_string()),
            )
        }
        "musicwheelspeed" => (
            "MusicWheelSpeed".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MUSIC_WHEEL_SPEED))
                .values(LuaOptionValues::Int(OPTION_MUSIC_WHEEL_SPEED_VALUES))
                .one_choice(),
            Some("MusicWheelSwitchSpeed".to_string()),
        ),
        "videorenderer" => (
            "VideoRenderer".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_VIDEO_RENDERER)).one_choice(),
            Some("VideoRenderers".to_string()),
        ),
        "globaloffsetseconds" => (
            "GlobalOffsetSeconds".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFFSET_MS))
                .values(LuaOptionValues::Number(OPTION_OFFSET_SECONDS_VALUES))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("GlobalOffsetSeconds".to_string()),
        ),
        "visualdelayseconds" => (
            "VisualDelaySeconds".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFFSET_MS))
                .values(LuaOptionValues::Number(OPTION_OFFSET_SECONDS_VALUES))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("VisualDelaySeconds".to_string()),
        ),
        "memorycards" => (
            "MemoryCards".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON))
                .values(LuaOptionValues::Bool(OPTION_FALSE_TRUE))
                .one_choice(),
            Some("MemoryCards".to_string()),
        ),
        "customsongsmaxseconds" => (
            "CustomSongsMaxSeconds".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_CUSTOM_SONG_SECONDS))
                .values(LuaOptionValues::Int(OPTION_CUSTOM_SONG_SECONDS_VALUES))
                .one_choice(),
            Some("CustomSongsMaxSeconds".to_string()),
        ),
        "customsongsmaxmegabytes" => (
            "CustomSongsMaxMegabytes".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_CUSTOM_SONG_MEGABYTES))
                .values(LuaOptionValues::Int(OPTION_CUSTOM_SONG_MEGABYTES_VALUES))
                .one_choice(),
            Some("CustomSongsMaxMegabytes".to_string()),
        ),
        "customsongsloadtimeout" => (
            "CustomSongsLoadTimeout".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_CUSTOM_SONG_TIMEOUT))
                .values(LuaOptionValues::Int(OPTION_CUSTOM_SONG_TIMEOUT_VALUES))
                .one_choice(),
            Some("CustomSongsLoadTimeout".to_string()),
        ),
        _ => (
            method_name.to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON)).one_choice(),
            None,
        ),
    };
    let row = create_compat_option_row_table(lua, &row_name, spec)?;
    if let Some(pref_name) = pref_name {
        set_pref_option_save(lua, &row, &pref_name)?;
    }
    Ok(row)
}

fn create_profileman_table(lua: &Lua) -> mlua::Result<Table> {
    let profileman = lua.create_table()?;
    profileman.set("__songlua_stats_prefix", "")?;
    let machine_profile = create_profile_table(lua, "Machine")?;
    let player_profiles = [
        create_profile_table(lua, "Player 1")?,
        create_profile_table(lua, "Player 2")?,
    ];
    let fallback_profile = create_profile_table(lua, "Player")?;
    let local_profile = create_profile_table(lua, "Local Profile")?;
    profileman.set(
        "GetMachineProfile",
        lua.create_function({
            let machine_profile = machine_profile.clone();
            move |_, _args: MultiValue| Ok(machine_profile.clone())
        })?,
    )?;
    profileman.set(
        "GetProfile",
        lua.create_function({
            let machine_profile = machine_profile.clone();
            let player_profiles = player_profiles.clone();
            let fallback_profile = fallback_profile.clone();
            move |_, args: MultiValue| {
                let profile = match method_arg(&args, 0).and_then(profile_slot_from_value) {
                    Some(SongLuaProfileSlot::Player(index)) => player_profiles[index].clone(),
                    Some(SongLuaProfileSlot::Machine) => machine_profile.clone(),
                    None => fallback_profile.clone(),
                };
                Ok(profile)
            }
        })?,
    )?;
    profileman.set(
        "GetProfileDir",
        lua.create_function(|lua, args: MultiValue| {
            let dir = match method_arg(&args, 0).and_then(profile_slot_from_value) {
                Some(SongLuaProfileSlot::Machine) => "/Save/MachineProfile/",
                _ => "",
            };
            Ok(Value::String(lua.create_string(dir)?))
        })?,
    )?;
    profileman.set(
        "GetPlayerName",
        lua.create_function(|lua, args: MultiValue| {
            let name = method_arg(&args, 0)
                .and_then(player_index_from_value)
                .map(|index| format!("Player {}", index + 1))
                .unwrap_or_default();
            Ok(Value::String(lua.create_string(&name)?))
        })?,
    )?;
    profileman.set(
        "IsPersistentProfile",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    profileman.set(
        "GetNumLocalProfiles",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    profileman.set(
        "LocalProfileIDToDir",
        lua.create_function(|lua, args: MultiValue| {
            let id = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(
                lua.create_string(format!("/Save/LocalProfiles/{id}/"))?,
            ))
        })?,
    )?;
    profileman.set(
        "GetLocalProfileIDFromIndex",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    profileman.set(
        "GetLocalProfileFromIndex",
        lua.create_function({
            let local_profile = local_profile.clone();
            move |_, _args: MultiValue| Ok(local_profile.clone())
        })?,
    )?;
    profileman.set(
        "GetLocalProfile",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    profileman.set(
        "GetLocalProfileIndexFromID",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    for method in ["GetLocalProfileIDs", "GetLocalProfileDisplayNames"] {
        profileman.set(
            method,
            lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
        )?;
    }
    for method in [
        "IsSongNew",
        "ProfileWasLoadedFromMemoryCard",
        "LastLoadWasTamperedOrCorrupt",
        "ProfileFromMemoryCardIsNew",
    ] {
        profileman.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(false))?,
        )?;
    }
    profileman.set(
        "GetSongNumTimesPlayed",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    profileman.set(
        "SaveMachineProfile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    for method in ["SaveProfile", "SaveLocalProfile"] {
        profileman.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(false)
            })?,
        )?;
    }
    profileman.set(
        "GetStatsPrefix",
        lua.create_function({
            let profileman = profileman.clone();
            move |lua, _args: MultiValue| {
                let prefix = profileman
                    .get::<Option<String>>("__songlua_stats_prefix")?
                    .unwrap_or_default();
                Ok(Value::String(lua.create_string(&prefix)?))
            }
        })?,
    )?;
    profileman.set(
        "SetStatsPrefix",
        lua.create_function({
            let profileman = profileman.clone();
            move |lua, args: MultiValue| {
                let prefix = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                profileman.set("__songlua_stats_prefix", prefix)?;
                note_song_lua_side_effect(lua)?;
                Ok(profileman.clone())
            }
        })?,
    )?;
    Ok(profileman)
}

#[derive(Clone, Copy)]
enum SongLuaProfileSlot {
    Player(usize),
    Machine,
}

fn profile_slot_from_value(value: &Value) -> Option<SongLuaProfileSlot> {
    if let Some(index) = player_index_from_value(value) {
        return Some(SongLuaProfileSlot::Player(index));
    }
    match value {
        Value::Integer(2) => return Some(SongLuaProfileSlot::Machine),
        Value::Number(value) if *value == 2.0 => return Some(SongLuaProfileSlot::Machine),
        _ => {}
    }
    let text = match value {
        Value::String(text) => text.to_str().ok()?,
        _ => return None,
    };
    match text.as_ref() {
        "ProfileSlot_Player1" => Some(SongLuaProfileSlot::Player(0)),
        "ProfileSlot_Player2" => Some(SongLuaProfileSlot::Player(1)),
        "ProfileSlot_Machine" => Some(SongLuaProfileSlot::Machine),
        _ => None,
    }
}

fn create_profile_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let profile = lua.create_table()?;
    profile.set("__songlua_display_name", name)?;
    profile.set("__songlua_last_score_name", name)?;
    profile.set("__songlua_guid", "")?;
    profile.set("__songlua_profile_dir", "")?;
    profile.set("__songlua_weight_pounds", 0.0_f32)?;
    profile.set("__songlua_voomax", 0.0_f32)?;
    profile.set("__songlua_birth_year", 0.0_f32)?;
    profile.set("__songlua_ignore_step_count_calories", false)?;
    profile.set("__songlua_is_male", true)?;
    profile.set("__songlua_goal_type", 0_i64)?;
    profile.set("__songlua_goal_calories", 0.0_f32)?;
    profile.set("__songlua_goal_seconds", 0.0_f32)?;
    profile.set("__songlua_calories_today", 0.0_f32)?;
    profile.set("__songlua_total_calories", 0.0_f32)?;
    profile.set("__songlua_user_table", lua.create_table()?)?;

    set_state_string_getter(lua, &profile, "GetDisplayName", "__songlua_display_name")?;
    set_state_string_setter(lua, &profile, "SetDisplayName", "__songlua_display_name")?;
    set_state_string_getter(lua, &profile, "GetGUID", "__songlua_guid")?;
    set_state_string_getter(lua, &profile, "GetProfileDir", "__songlua_profile_dir")?;
    set_state_string_getter(
        lua,
        &profile,
        "GetLastUsedHighScoreName",
        "__songlua_last_score_name",
    )?;
    set_state_string_setter(
        lua,
        &profile,
        "SetLastUsedHighScoreName",
        "__songlua_last_score_name",
    )?;

    set_string_method(lua, &profile, "GetType", "ProfileType_Normal")?;
    profile.set(
        "GetPriority",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    profile.set(
        "AddScreenshot",
        lua.create_function({
            let profile = profile.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(profile.clone())
            }
        })?,
    )?;
    profile.set(
        "GetAllUsedHighScoreNames",
        lua.create_function({
            let profile = profile.clone();
            move |lua, _args: MultiValue| {
                let names = lua.create_table()?;
                let name = profile
                    .get::<Option<String>>("__songlua_last_score_name")?
                    .unwrap_or_default();
                if !name.is_empty() {
                    names.raw_set(1, name)?;
                }
                Ok(names)
            }
        })?,
    )?;
    profile.set(
        "GetCharacter",
        lua.create_function({
            let profile = profile.clone();
            move |_, _args: MultiValue| profile.get::<Value>("__songlua_character")
        })?,
    )?;
    profile.set(
        "SetCharacter",
        lua.create_function({
            let profile = profile.clone();
            move |lua, args: MultiValue| {
                let character = method_arg(&args, 0).cloned().unwrap_or(Value::Nil);
                profile.set("__songlua_character", character)?;
                note_song_lua_side_effect(lua)?;
                Ok(profile.clone())
            }
        })?,
    )?;
    set_state_number_getter(lua, &profile, "GetWeightPounds", "__songlua_weight_pounds")?;
    set_state_number_setter(lua, &profile, "SetWeightPounds", "__songlua_weight_pounds")?;
    set_state_number_getter(lua, &profile, "GetVoomax", "__songlua_voomax")?;
    set_state_number_setter(lua, &profile, "SetVoomax", "__songlua_voomax")?;
    set_state_number_getter(lua, &profile, "GetBirthYear", "__songlua_birth_year")?;
    set_state_number_setter(lua, &profile, "SetBirthYear", "__songlua_birth_year")?;
    set_state_bool_getter(
        lua,
        &profile,
        "GetIgnoreStepCountCalories",
        "__songlua_ignore_step_count_calories",
    )?;
    set_state_bool_setter(
        lua,
        &profile,
        "SetIgnoreStepCountCalories",
        "__songlua_ignore_step_count_calories",
    )?;
    set_state_bool_getter(lua, &profile, "GetIsMale", "__songlua_is_male")?;
    set_state_bool_setter(lua, &profile, "SetIsMale", "__songlua_is_male")?;
    set_state_number_getter(lua, &profile, "GetGoalCalories", "__songlua_goal_calories")?;
    set_state_number_setter(lua, &profile, "SetGoalCalories", "__songlua_goal_calories")?;
    set_state_number_getter(lua, &profile, "GetGoalSeconds", "__songlua_goal_seconds")?;
    set_state_number_setter(lua, &profile, "SetGoalSeconds", "__songlua_goal_seconds")?;
    profile.set(
        "GetGoalType",
        lua.create_function({
            let profile = profile.clone();
            move |_, _args: MultiValue| profile.get::<Value>("__songlua_goal_type")
        })?,
    )?;
    profile.set(
        "SetGoalType",
        lua.create_function({
            let profile = profile.clone();
            move |lua, args: MultiValue| {
                let goal = method_arg(&args, 0).cloned().unwrap_or(Value::Integer(0));
                profile.set("__songlua_goal_type", goal)?;
                note_song_lua_side_effect(lua)?;
                Ok(profile.clone())
            }
        })?,
    )?;

    for method in [
        "GetAge",
        "GetTotalNumSongsPlayed",
        "GetNumTotalSongsPlayed",
        "GetTotalSessions",
        "GetTotalSessionSeconds",
        "GetTotalGameplaySeconds",
        "GetSongNumTimesPlayed",
        "GetNumToasties",
        "GetTotalTapsAndHolds",
        "GetTotalJumps",
        "GetTotalHolds",
        "GetTotalRolls",
        "GetTotalMines",
        "GetTotalHands",
        "GetTotalLifts",
        "GetTotalDancePoints",
        "GetTotalStepsWithTopGrade",
        "GetTotalTrailsWithTopGrade",
    ] {
        profile.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
        )?;
    }
    for method in [
        "CalculateCaloriesFromHeartRate",
        "GetSongsActual",
        "GetCoursesActual",
        "GetSongsPossible",
        "GetCoursesPossible",
        "GetSongsPercentComplete",
        "GetCoursesPercentComplete",
        "GetSongsAndCoursesPercentCompleteAllDifficulties",
    ] {
        profile.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
        )?;
    }
    for method in ["IsCodeUnlocked", "HasPassedAnyStepsInSong"] {
        profile.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(false))?,
        )?;
    }
    for method in [
        "GetMostPopularSong",
        "GetMostPopularCourse",
        "GetLastPlayedSong",
        "GetLastPlayedCourse",
    ] {
        profile.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    set_state_number_getter(
        lua,
        &profile,
        "GetCaloriesBurnedToday",
        "__songlua_calories_today",
    )?;
    set_state_number_getter(
        lua,
        &profile,
        "GetTotalCaloriesBurned",
        "__songlua_total_calories",
    )?;
    profile.set(
        "GetDisplayTotalCaloriesBurned",
        lua.create_function({
            let profile = profile.clone();
            move |lua, _args: MultiValue| {
                let calories = profile
                    .get::<Option<f32>>("__songlua_total_calories")?
                    .unwrap_or(0.0)
                    .max(0.0) as i64;
                Ok(Value::String(lua.create_string(format!("{calories} Cal"))?))
            }
        })?,
    )?;
    profile.set(
        "AddCaloriesToDailyTotal",
        lua.create_function({
            let profile = profile.clone();
            move |lua, args: MultiValue| {
                let calories = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let today = profile
                    .get::<Option<f32>>("__songlua_calories_today")?
                    .unwrap_or(0.0)
                    + calories;
                let total = profile
                    .get::<Option<f32>>("__songlua_total_calories")?
                    .unwrap_or(0.0)
                    + calories;
                profile.set("__songlua_calories_today", today)?;
                profile.set("__songlua_total_calories", total)?;
                note_song_lua_side_effect(lua)?;
                Ok(profile.clone())
            }
        })?,
    )?;
    profile.set(
        "GetHighScoreList",
        lua.create_function(|lua, _args: MultiValue| create_high_score_list_table(lua))?,
    )?;
    profile.set(
        "GetHighScoreListIfExists",
        lua.create_function(|lua, _args: MultiValue| create_high_score_list_table(lua))?,
    )?;
    profile.set(
        "GetCategoryHighScoreList",
        lua.create_function(|lua, _args: MultiValue| create_high_score_list_table(lua))?,
    )?;
    profile.set(
        "GetUserTable",
        lua.create_function({
            let profile = profile.clone();
            move |_, _args: MultiValue| profile.get::<Table>("__songlua_user_table")
        })?,
    )?;
    profile.set(
        "get_songs",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    Ok(profile)
}

fn set_state_string_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                let value = table.get::<Option<String>>(key)?.unwrap_or_default();
                Ok(Value::String(lua.create_string(&value)?))
            }
        })?,
    )
}

fn set_state_string_setter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}

fn set_state_number_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |_, _args: MultiValue| Ok(table.get::<Option<f32>>(key)?.unwrap_or(0.0))
        })?,
    )
}

fn set_state_number_setter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}

fn set_state_bool_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |_, _args: MultiValue| Ok(table.get::<Option<bool>>(key)?.unwrap_or(false))
        })?,
    )
}

fn set_state_bool_setter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(false);
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}

fn create_statsman_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let statsman = lua.create_table()?;
    let stage_stats = create_stage_stats_table(lua, context)?;
    for name in [
        "GetCurStageStats",
        "GetAccumPlayedStageStats",
        "GetFinalEvalStageStats",
    ] {
        statsman.set(
            name,
            lua.create_function({
                let stage_stats = stage_stats.clone();
                move |_, _args: MultiValue| Ok(stage_stats.clone())
            })?,
        )?;
    }
    statsman.set(
        "GetPlayedStageStats",
        lua.create_function({
            let stage_stats = stage_stats.clone();
            move |_, args: MultiValue| {
                let ago = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(1);
                Ok(if ago == 1 {
                    Value::Table(stage_stats.clone())
                } else {
                    Value::Nil
                })
            }
        })?,
    )?;
    statsman.set("Reset", lua.create_function(|_, _args: MultiValue| Ok(()))?)?;
    statsman.set(
        "GetStagesPlayed",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    for name in [
        "GetFinalGrade",
        "GetBestGrade",
        "GetWorstGrade",
        "GetBestFinalGrade",
    ] {
        set_string_method(lua, &statsman, name, "Grade_Tier07")?;
    }
    Ok(statsman)
}

fn create_stage_stats_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let stage_stats = lua.create_table()?;
    let player_stats = [
        create_player_stage_stats_table(
            lua,
            "Player",
            context.players[0].clone(),
            context.song_dir.as_path(),
        )?,
        create_player_stage_stats_table(
            lua,
            "Player",
            context.players[1].clone(),
            context.song_dir.as_path(),
        )?,
    ];
    let multi_player_stats = create_player_stage_stats_table(
        lua,
        "MultiPlayer",
        context.players[0].clone(),
        context.song_dir.as_path(),
    )?;
    let song = create_song_table(lua, context)?;
    stage_stats.set(
        "GetPlayerStageStats",
        lua.create_function(move |_, args: MultiValue| {
            let index = method_arg(&args, 0)
                .and_then(player_index_from_value)
                .unwrap_or(0);
            Ok(player_stats[index].clone())
        })?,
    )?;
    stage_stats.set(
        "GetMultiPlayerStageStats",
        lua.create_function(move |_, _args: MultiValue| Ok(multi_player_stats.clone()))?,
    )?;
    for name in ["GetPlayedSongs", "GetPossibleSongs"] {
        stage_stats.set(
            name,
            lua.create_function({
                let song = song.clone();
                move |lua, _args: MultiValue| {
                    let table = lua.create_table()?;
                    table.raw_set(1, song.clone())?;
                    Ok(table)
                }
            })?,
        )?;
    }
    stage_stats.set(
        "__songlua_total_steps_seconds",
        context.music_length_seconds.max(0.0),
    )?;
    stage_stats.set(
        "GetGameplaySeconds",
        lua.create_function({
            let total_seconds = context.music_length_seconds.max(0.0);
            move |_, _args: MultiValue| Ok(total_seconds)
        })?,
    )?;
    stage_stats.set(
        "GetTotalPossibleStepsSeconds",
        lua.create_function({
            let total_seconds = context.music_length_seconds.max(0.0);
            move |_, _args: MultiValue| Ok(total_seconds)
        })?,
    )?;
    stage_stats.set(
        "GetStepsSeconds",
        lua.create_function({
            let total_seconds = context.music_length_seconds.max(0.0);
            move |_, _args: MultiValue| Ok(total_seconds)
        })?,
    )?;
    stage_stats.set(
        "GetStageIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    set_string_method(lua, &stage_stats, "GetStage", "Stage_1st")?;
    stage_stats.set(
        "AllFailed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stage_stats.set(
        "OnePassed",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    stage_stats.set(
        "PlayerHasHighScore",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stage_stats.set(
        "GetEarnedExtraStage",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stage_stats.set(
        "GaveUp",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(stage_stats)
}

fn create_player_stage_stats_table(
    lua: &Lua,
    high_score_name: &str,
    player: SongLuaPlayerContext,
    song_dir: &Path,
) -> mlua::Result<Table> {
    let stats = lua.create_table()?;
    stats.set("__songlua_failed", false)?;
    stats.set("__songlua_score", 0_i64)?;
    stats.set("__songlua_cur_max_score", 0_i64)?;
    stats.set("__songlua_actual_dance_points", 0_i64)?;
    stats.set("__songlua_possible_dance_points", 1_i64)?;
    let high_score = create_high_score_table(lua, high_score_name)?;
    let played_steps = lua.create_table()?;
    let possible_steps = lua.create_table()?;
    let steps = create_steps_table(lua, player.difficulty, player.display_bpms, song_dir)?;
    played_steps.raw_set(1, steps.clone())?;
    possible_steps.raw_set(1, steps)?;
    stats.set(
        "GetCaloriesBurned",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetNumControllerSteps",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetSurvivalSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetCurrentScoreMultiplier",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    stats.set(
        "GetCurMaxScore",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<i64>>("__songlua_cur_max_score")?
                    .unwrap_or(0))
            }
        })?,
    )?;
    set_string_method(lua, &stats, "GetGrade", "Grade_Tier07")?;
    stats.set(
        "GetFailed",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<bool>>("__songlua_failed")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    stats.set(
        "FailPlayer",
        lua.create_function({
            let stats = stats.clone();
            move |lua, _args: MultiValue| {
                stats.set("__songlua_failed", true)?;
                note_song_lua_side_effect(lua)?;
                Ok(stats.clone())
            }
        })?,
    )?;
    stats.set(
        "GetScore",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats.get::<Option<i64>>("__songlua_score")?.unwrap_or(0))
            }
        })?,
    )?;
    stats.set(
        "SetScore",
        lua.create_function({
            let stats = stats.clone();
            move |lua, args: MultiValue| {
                if let Some(score) = method_arg(&args, 0).cloned().and_then(read_i32_value)
                    && score >= 0
                {
                    stats.set("__songlua_score", score as i64)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(stats.clone())
            }
        })?,
    )?;
    stats.set(
        "SetCurMaxScore",
        lua.create_function({
            let stats = stats.clone();
            move |lua, args: MultiValue| {
                if let Some(score) = method_arg(&args, 0).cloned().and_then(read_i32_value)
                    && score >= 0
                {
                    stats.set("__songlua_cur_max_score", score as i64)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(stats.clone())
            }
        })?,
    )?;
    stats.set(
        "SetDancePointLimits",
        lua.create_function({
            let stats = stats.clone();
            move |lua, args: MultiValue| {
                let actual = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(0)
                    .max(0) as i64;
                let possible = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(1)
                    .max(1) as i64;
                stats.set("__songlua_possible_dance_points", possible)?;
                stats.set("__songlua_actual_dance_points", actual.min(possible))?;
                note_song_lua_side_effect(lua)?;
                Ok(stats.clone())
            }
        })?,
    )?;
    stats.set(
        "GetPercentDancePoints",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                let actual = stats
                    .get::<Option<i64>>("__songlua_actual_dance_points")?
                    .unwrap_or(0);
                let possible = stats
                    .get::<Option<i64>>("__songlua_possible_dance_points")?
                    .unwrap_or(1)
                    .max(1);
                Ok(actual as f64 / possible as f64)
            }
        })?,
    )?;
    stats.set(
        "GetActualDancePoints",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<i64>>("__songlua_actual_dance_points")?
                    .unwrap_or(0))
            }
        })?,
    )?;
    stats.set(
        "GetPossibleDancePoints",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<i64>>("__songlua_possible_dance_points")?
                    .unwrap_or(1))
            }
        })?,
    )?;
    stats.set(
        "GetCurrentPossibleDancePoints",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<i64>>("__songlua_possible_dance_points")?
                    .unwrap_or(1))
            }
        })?,
    )?;
    stats.set(
        "GetCurrentLife",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_INITIAL_LIFE))?,
    )?;
    stats.set(
        "GetCurrentCombo",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetCurrentMissCombo",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetLifeRemainingSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetAliveSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetLifeRecord",
        lua.create_function(|lua, args: MultiValue| {
            let samples = method_arg(&args, 1)
                .cloned()
                .and_then(read_i32_value)
                .unwrap_or(GRAPH_DISPLAY_VALUE_RESOLUTION as i32)
                .max(1) as usize;
            create_life_record_table(lua, samples, SONG_LUA_INITIAL_LIFE)
        })?,
    )?;
    stats.set(
        "GetTapNoteScores",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetHoldNoteScores",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "FullCombo",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stats.set(
        "MaxCombo",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetLessonScoreActual",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetLessonScoreNeeded",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetPlayedSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(played_steps.clone()))?,
    )?;
    stats.set(
        "GetPossibleSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(possible_steps.clone()))?,
    )?;
    stats.set(
        "GetComboList",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    stats.set(
        "GetRadarActual",
        lua.create_function(|lua, _args: MultiValue| create_stage_radar_values_table(lua, 0.0))?,
    )?;
    stats.set(
        "GetRadarPossible",
        lua.create_function(|lua, _args: MultiValue| create_stage_radar_values_table(lua, 1.0))?,
    )?;
    stats.set(
        "GetRadarValues",
        lua.create_function(|lua, _args: MultiValue| create_stage_radar_values_table(lua, 0.0))?,
    )?;
    stats.set(
        "GetHighScore",
        lua.create_function(move |_, _args: MultiValue| Ok(high_score.clone()))?,
    )?;
    stats.set(
        "GetMachineHighScoreIndex",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    stats.set(
        "GetPersonalHighScoreIndex",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    set_string_method(lua, &stats, "GetStageAward", "StageAward_None")?;
    set_string_method(lua, &stats, "GetPeakComboAward", "PeakComboAward_None")?;
    stats.set(
        "IsDisqualified",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stats.set(
        "GetPercentageOfTaps",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    set_string_method(
        lua,
        &stats,
        "GetBestFullComboTapNoteScore",
        "TapNoteScore_None",
    )?;
    stats.set(
        "FullComboOfScore",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stats.set(
        "GetSongsPassed",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetSongsPlayed",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    Ok(stats)
}

fn create_life_record_table(lua: &Lua, samples: usize, life: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let value = life.clamp(0.0, 1.0);
    for index in 1..=samples {
        table.raw_set(index, value)?;
    }
    Ok(table)
}

fn create_stage_radar_values_table(lua: &Lua, fallback: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetValue",
        lua.create_function(move |_, _args: MultiValue| Ok(fallback))?,
    )?;
    Ok(table)
}

fn create_ex_judgment_counts(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in [
        "W0",
        "W1",
        "W2",
        "W3",
        "W4",
        "W5",
        "Miss",
        "Hands",
        "Holds",
        "Mines",
        "Rolls",
        "totalHands",
        "totalHolds",
        "totalMines",
        "totalRolls",
    ] {
        table.set(name, 0_i64)?;
    }
    Ok(table)
}

fn create_high_score_list_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let high_scores = lua.create_table()?;
    high_scores.raw_set(1, create_high_score_table(lua, "Machine")?)?;
    let high_scores_for_lookup = high_scores.clone();
    table.set(
        "GetHighScores",
        lua.create_function({
            let high_scores = high_scores.clone();
            move |_, _args: MultiValue| Ok(high_scores.clone())
        })?,
    )?;
    table.set(
        "GetHighestScoreOfName",
        lua.create_function(move |_, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            for score in high_scores_for_lookup.sequence_values::<Table>() {
                let score = score?;
                let score_name = score
                    .get::<Function>("GetName")?
                    .call::<String>(score.clone())?;
                if score_name == name {
                    return Ok(Value::Table(score));
                }
            }
            Ok(Value::Nil)
        })?,
    )?;
    table.set(
        "GetRankOfName",
        lua.create_function(move |_, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(0_i64);
            };
            for (index, score) in high_scores.sequence_values::<Table>().enumerate() {
                let score = score?;
                let score_name = score
                    .get::<Function>("GetName")?
                    .call::<String>(score.clone())?;
                if score_name == name {
                    return Ok(index as i64 + 1);
                }
            }
            Ok(0_i64)
        })?,
    )?;
    Ok(table)
}

fn create_high_score_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let score = lua.create_table()?;
    set_string_method(lua, &score, "GetName", name)?;
    set_string_method(lua, &score, "GetGrade", "Grade_Tier07")?;
    set_string_method(lua, &score, "GetDate", "")?;
    set_string_method(lua, &score, "GetModifiers", "")?;
    score.set(
        "GetScore",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    score.set(
        "GetPercentDP",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    score.set(
        "GetPercentDancePoints",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    score.set(
        "GetTapNoteScore",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    score.set(
        "GetHoldNoteScore",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    score.set(
        "GetMaxCombo",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    score.set(
        "GetSurvivalSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    score.set(
        "IsFillInMarker",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    score.set(
        "GetRadarValues",
        lua.create_function(|lua, _args: MultiValue| create_stage_radar_values_table(lua, 0.0))?,
    )?;
    set_string_method(lua, &score, "GetStageAward", "StageAward_None")?;
    set_string_method(lua, &score, "GetPeakComboAward", "PeakComboAward_None")?;
    Ok(score)
}

fn create_display_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let display = lua.create_table()?;
    let width = context.screen_width.max(1.0).round() as i32;
    let height = context.screen_height.max(1.0).round() as i32;
    let specs = create_display_specs_table(lua, width, height)?;

    display.set(
        "GetDisplayWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(width))?,
    )?;
    display.set(
        "GetDisplayHeight",
        lua.create_function(move |_, _args: MultiValue| Ok(height))?,
    )?;
    display.set(
        "GetFPS",
        lua.create_function(|_, _args: MultiValue| Ok(60))?,
    )?;
    display.set("GetVPF", lua.create_function(|_, _args: MultiValue| Ok(1))?)?;
    display.set(
        "GetCumFPS",
        lua.create_function(|_, _args: MultiValue| Ok(60))?,
    )?;
    display.set(
        "GetDisplaySpecs",
        lua.create_function(move |_, _args: MultiValue| Ok(specs.clone()))?,
    )?;
    display.set(
        "SupportsRenderToTexture",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    display.set(
        "SupportsFullscreenBorderlessWindow",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(display)
}

fn create_display_specs_table(lua: &Lua, width: i32, height: i32) -> mlua::Result<Table> {
    let specs = lua.create_table()?;
    specs.raw_set(1, create_display_spec_table(lua, width, height)?)?;
    let mt = lua.create_table()?;
    mt.set(
        "__tostring",
        lua.create_function(|_, _self: Value| Ok("DisplaySpecs: songlua"))?,
    )?;
    let _ = specs.set_metatable(Some(mt));
    Ok(specs)
}

fn create_display_spec_table(lua: &Lua, width: i32, height: i32) -> mlua::Result<Table> {
    let spec = lua.create_table()?;
    let mode = create_display_mode_table(lua, width, height)?;
    let modes = lua.create_table()?;
    modes.raw_set(1, mode.clone())?;
    set_string_method(lua, &spec, "GetId", "Default")?;
    set_string_method(lua, &spec, "GetName", "Default Display")?;
    spec.set(
        "GetSupportedModes",
        lua.create_function({
            let modes = modes.clone();
            move |_, _args: MultiValue| Ok(modes.clone())
        })?,
    )?;
    spec.set(
        "IsVirtual",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    spec.set(
        "GetCurrentMode",
        lua.create_function(move |_, _args: MultiValue| Ok(mode.clone()))?,
    )?;
    Ok(spec)
}

fn create_display_mode_table(lua: &Lua, width: i32, height: i32) -> mlua::Result<Table> {
    let mode = lua.create_table()?;
    mode.set(
        "GetWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(width))?,
    )?;
    mode.set(
        "GetHeight",
        lua.create_function(move |_, _args: MultiValue| Ok(height))?,
    )?;
    mode.set(
        "GetRefreshRate",
        lua.create_function(|_, _args: MultiValue| Ok(60))?,
    )?;
    Ok(mode)
}

fn create_memcardman_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetCardState",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("MemoryCardState_none")?))
        })?,
    )?;
    table.set(
        "GetName",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    for name in ["MountCard", "UnmountCard"] {
        table.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(false)
            })?,
        )?;
    }
    Ok(table)
}

fn create_unlockman_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in [
        "GetNumUnlocks",
        "GetNumUnlocked",
        "GetPoints",
        "GetPointsForProfile",
        "GetPointsUntilNextUnlock",
    ] {
        table.set(name, lua.create_function(|_, _args: MultiValue| Ok(0_i64))?)?;
    }
    table.set(
        "AnyUnlocksToCelebrate",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    table.set(
        "GetUnlockEntryIndexToCelebrate",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    for name in ["FindEntryID", "GetUnlockEntry"] {
        table.set(
            name,
            lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    table.set(
        "GetSongsUnlockedByEntryID",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    table.set(
        "GetStepsUnlockedByEntryID",
        lua.create_function(|lua, _args: MultiValue| {
            Ok((lua.create_table()?, lua.create_table()?))
        })?,
    )?;
    for name in [
        "PreferUnlockEntryID",
        "UnlockEntryID",
        "UnlockEntryIndex",
        "LockEntryID",
        "LockEntryIndex",
    ] {
        table.set(
            name,
            lua.create_function({
                let table = table.clone();
                move |lua, _args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    Ok(table.clone())
                }
            })?,
        )?;
    }
    table.set(
        "IsSongLocked",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    table.set(
        "IsCourseLocked",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    table.set(
        "IsStepsLocked",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    Ok(table)
}

fn create_hooks_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetArchName",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(song_lua_arch_name())?))
        })?,
    )?;
    table.set(
        "GetClipboard",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    table.set(
        "SetClipboard",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    for method in ["OpenFile", "OpenURL", "RestartProgram"] {
        table.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(false)
            })?,
        )?;
    }
    Ok(table)
}

fn song_lua_arch_name() -> &'static str {
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

fn create_noteskin_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let noteskin = lua.create_table()?;
    let default_noteskin = song_lua_default_noteskin_name(context);

    let default_metric_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetric",
        lua.create_function(
            move |lua, (_self, element, value): (Table, String, String)| {
                let Some(metric) = crate::game::parsing::noteskin::song_lua_noteskin_metric(
                    &default_metric_skin,
                    &element,
                    &value,
                ) else {
                    return Ok(Value::Nil);
                };
                Ok(Value::String(lua.create_string(&metric)?))
            },
        )?,
    )?;
    noteskin.set(
        "GetMetricForNoteSkin",
        lua.create_function(
            move |lua, (_self, element, value, skin): (Table, String, String, String)| {
                let Some(metric) = crate::game::parsing::noteskin::song_lua_noteskin_metric(
                    &skin, &element, &value,
                ) else {
                    return Ok(Value::Nil);
                };
                Ok(Value::String(lua.create_string(&metric)?))
            },
        )?,
    )?;

    let default_metric_f_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricF",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(crate::game::parsing::noteskin::song_lua_noteskin_metric_f(
                &default_metric_f_skin,
                &element,
                &value,
            )
            .unwrap_or(0.0_f32))
        })?,
    )?;
    noteskin.set(
        "GetMetricFForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(crate::game::parsing::noteskin::song_lua_noteskin_metric_f(
                    &skin, &element, &value,
                )
                .unwrap_or(0.0_f32))
            },
        )?,
    )?;

    let default_metric_i_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricI",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(song_lua_noteskin_metric_i(
                &default_metric_i_skin,
                &element,
                &value,
            ))
        })?,
    )?;
    noteskin.set(
        "GetMetricIForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(song_lua_noteskin_metric_i(&skin, &element, &value))
            },
        )?,
    )?;

    let default_metric_b_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricB",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(crate::game::parsing::noteskin::song_lua_noteskin_metric_b(
                &default_metric_b_skin,
                &element,
                &value,
            )
            .unwrap_or(false))
        })?,
    )?;
    noteskin.set(
        "GetMetricBForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(crate::game::parsing::noteskin::song_lua_noteskin_metric_b(
                    &skin, &element, &value,
                )
                .unwrap_or(false))
            },
        )?,
    )?;

    noteskin.set(
        "GetMetricA",
        lua.create_function(
            move |lua, (_self, _element, _value): (Table, String, String)| {
                song_lua_noteskin_metric_a(lua)
            },
        )?,
    )?;
    noteskin.set(
        "GetMetricAForNoteSkin",
        lua.create_function(
            move |lua, (_self, _element, _value, _skin): (Table, String, String, String)| {
                song_lua_noteskin_metric_a(lua)
            },
        )?,
    )?;

    let default_path_skin = default_noteskin.clone();
    noteskin.set(
        "GetPath",
        lua.create_function(
            move |lua, (_self, button, element): (Table, String, String)| {
                let path = song_lua_noteskin_path(&default_path_skin, &button, &element);
                Ok(Value::String(lua.create_string(&path)?))
            },
        )?,
    )?;
    noteskin.set(
        "GetPathForNoteSkin",
        lua.create_function(
            move |lua, (_self, button, element, skin): (Table, String, String, String)| {
                let path = song_lua_noteskin_path(&skin, &button, &element);
                Ok(Value::String(lua.create_string(&path)?))
            },
        )?,
    )?;

    let default_load_skin = default_noteskin.clone();
    noteskin.set(
        "LoadActor",
        lua.create_function(
            move |lua, (_self, button, element): (Table, String, String)| {
                song_lua_noteskin_actor(lua, &default_load_skin, &button, &element)
            },
        )?,
    )?;
    noteskin.set(
        "LoadActorForNoteSkin",
        lua.create_function(
            move |lua, (_self, button, element, skin): (Table, String, String, String)| {
                song_lua_noteskin_actor(lua, &skin, &button, &element)
            },
        )?,
    )?;

    noteskin.set(
        "DoesNoteSkinExist",
        lua.create_function(|_, (_self, skin): (Table, String)| {
            Ok(crate::game::parsing::noteskin::song_lua_noteskin_exists(
                &skin,
            ))
        })?,
    )?;
    noteskin.set(
        "GetNoteSkinNames",
        lua.create_function(|lua, _args: MultiValue| {
            let names = crate::game::parsing::noteskin::discover_itg_skins("dance");
            let table = lua.create_table()?;
            for (idx, name) in names.into_iter().enumerate() {
                table.raw_set(idx + 1, name)?;
            }
            Ok(table)
        })?,
    )?;
    noteskin.set(
        "HasVariants",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    noteskin.set(
        "IsNoteSkinVariant",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    noteskin.set(
        "GetVariantNamesForNoteSkin",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    Ok(noteskin)
}

fn song_lua_default_noteskin_name(context: &SongLuaCompileContext) -> String {
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
        .unwrap_or_else(|| crate::game::profile::NoteSkin::default().to_string())
}

fn song_lua_noteskin_path(skin: &str, button: &str, element: &str) -> String {
    crate::game::parsing::noteskin::song_lua_noteskin_resolve_path(skin, button, element)
        .map(|path| file_path_string(path.as_path()))
        .unwrap_or_default()
}

fn song_lua_noteskin_metric_i(skin: &str, element: &str, value: &str) -> i64 {
    let Some(metric) =
        crate::game::parsing::noteskin::song_lua_noteskin_metric(skin, element, value)
    else {
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

fn song_lua_noteskin_metric_a(lua: &Lua) -> mlua::Result<Function> {
    lua.create_function(|_, actor: Table| Ok(Value::Table(actor)))
}

fn song_lua_noteskin_actor(
    lua: &Lua,
    skin: &str,
    button: &str,
    element: &str,
) -> mlua::Result<Table> {
    let resolved =
        crate::game::parsing::noteskin::song_lua_noteskin_resolve_path(skin, button, element);
    let sprite_path = resolved
        .as_ref()
        .filter(|path| is_song_lua_image_path(path));
    let actor = create_dummy_actor(
        lua,
        if sprite_path.is_some() {
            "Sprite"
        } else {
            "Actor"
        },
    )?;
    actor.set("__songlua_noteskin_name", skin.trim().to_ascii_lowercase())?;
    actor.set("__songlua_noteskin_button", button)?;
    actor.set("__songlua_noteskin_element", element)?;
    if let Some(path) = sprite_path {
        actor.set("Texture", file_path_string(path.as_path()))?;
    }
    Ok(actor)
}

fn theme_string(section: &str, name: &str) -> String {
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

fn theme_has_string(section: &str, name: &str) -> bool {
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

fn theme_pref_default(lua: &Lua, name: &str) -> mlua::Result<Value> {
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
        return Ok(Value::Integer(match lower.as_str() {
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
        return Ok(Value::String(lua.create_string(value)?));
    }
    Ok(Value::Boolean(matches!(lower.as_str(), "useimagecache")))
}

fn create_player_state_table(
    lua: &Lua,
    player: SongLuaPlayerContext,
    player_index: usize,
    song_runtime: &Table,
) -> mlua::Result<Table> {
    let controller = if player.enabled {
        "PlayerController_Human"
    } else {
        "PlayerController_Autoplay"
    };
    let health_state = if player.enabled {
        "HealthState_Alive"
    } else {
        "HealthState_Dead"
    };
    let player_number = player_number_name(player_index);
    let options = create_player_options_table(lua, player)?;
    let song_position = create_song_position_table(lua, song_runtime)?;
    let table = lua.create_table()?;
    table.set("__songlua_player_options_string", String::new())?;
    set_string_method(lua, &table, "GetPlayerController", controller)?;
    set_string_method(lua, &table, "GetHealthState", health_state)?;
    set_string_method(lua, &table, "GetPlayerNumber", player_number)?;
    let options_for_current = options.clone();
    let options_for_set = options.clone();
    table.set(
        "GetPlayerOptions",
        lua.create_function(move |_, _args: MultiValue| Ok(options.clone()))?,
    )?;
    table.set(
        "GetCurrentPlayerOptions",
        lua.create_function(move |_, _args: MultiValue| Ok(options_for_current.clone()))?,
    )?;
    table.set(
        "GetSongPosition",
        lua.create_function(move |_, _args: MultiValue| Ok(song_position.clone()))?,
    )?;
    table.set(
        "GetPlayerOptionsString",
        lua.create_function(|_, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(String::new());
            };
            Ok(owner
                .get::<Option<String>>("__songlua_player_options_string")?
                .unwrap_or_default())
        })?,
    )?;
    table.set(
        "GetPlayerOptionsArray",
        lua.create_function(|lua, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return lua.create_table();
            };
            create_player_options_array(lua, &owner)
        })?,
    )?;
    table.set(
        "SetPlayerOptions",
        lua.create_function({
            move |lua, args: MultiValue| {
                let Some(owner) = args.front().and_then(|value| match value {
                    Value::Table(table) => Some(table.clone()),
                    _ => None,
                }) else {
                    return Ok(());
                };
                let options_text = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                owner.set("__songlua_player_options_string", options_text.clone())?;
                apply_player_options_string(lua, &options_for_set, &options_text)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    Ok(table)
}

fn create_player_options_array(lua: &Lua, owner: &Table) -> mlua::Result<Table> {
    let text = owner
        .get::<Option<String>>("__songlua_player_options_string")?
        .unwrap_or_default();
    let table = lua.create_table()?;
    for (index, option) in text
        .split(',')
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .enumerate()
    {
        table.raw_set(index + 1, option)?;
    }
    Ok(table)
}

fn create_steps_table(
    lua: &Lua,
    difficulty: SongLuaDifficulty,
    display_bpms: [f32; 2],
    song_dir: &Path,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetDifficulty", difficulty.sm_name())?;
    set_string_method(lua, &table, "GetStepsType", "StepsType_Dance_Single")?;
    set_string_method(lua, &table, "GetDescription", "")?;
    set_string_method(lua, &table, "GetChartName", "")?;
    set_string_method(lua, &table, "GetAuthorCredit", "")?;
    set_string_method(lua, &table, "GetCredit", "")?;
    set_string_method(
        lua,
        &table,
        "GetFilename",
        &song_simfile_path(song_dir)
            .map(|path| file_path_string(path.as_path()))
            .unwrap_or_default(),
    )?;
    set_string_method(
        lua,
        &table,
        "GetMusicPath",
        &song_music_path(song_dir)
            .map(|path| file_path_string(path.as_path()))
            .unwrap_or_default(),
    )?;
    let meter = difficulty_meter(difficulty);
    table.set(
        "GetMeter",
        lua.create_function(move |_, _args: MultiValue| Ok(meter))?,
    )?;
    let timing = create_timing_table(lua, display_bpms)?;
    let display_bpms = create_display_bpms_table(lua, display_bpms)?;
    table.set(
        "GetDisplayBpms",
        lua.create_function(move |_, _args: MultiValue| Ok(display_bpms.clone()))?,
    )?;
    table.set(
        "GetTimingData",
        lua.create_function(move |_, _args: MultiValue| Ok(timing.clone()))?,
    )?;
    let radar = create_radar_values_table(lua)?;
    table.set(
        "GetRadarValues",
        lua.create_function(move |_, _args: MultiValue| Ok(radar.clone()))?,
    )?;
    set_string_method(lua, &table, "GetDisplayBPMType", "DISPLAY_BPM_ACTUAL")?;
    Ok(table)
}

fn create_display_bpms_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, bpms[0])?;
    table.raw_set(2, bpms[1])?;
    Ok(table)
}

fn display_bpms_for_args(
    args: &MultiValue,
    fallback: [f32; 2],
    default_rate: f32,
) -> mlua::Result<([f32; 2], f32)> {
    let rate = args
        .get(2)
        .cloned()
        .and_then(read_f32)
        .unwrap_or(default_rate)
        .max(f32::EPSILON);
    let bpms = args
        .get(1)
        .and_then(display_bpms_from_value)
        .transpose()?
        .unwrap_or(fallback);
    Ok(([bpms[0] * rate, bpms[1] * rate], rate))
}

fn display_bpms_from_value(value: &Value) -> Option<mlua::Result<[f32; 2]>> {
    let Value::Table(table) = value else {
        return None;
    };
    Some(display_bpms_from_table(table))
}

fn display_bpms_from_table(table: &Table) -> mlua::Result<[f32; 2]> {
    if let Some(bpms) = table
        .get::<Option<Function>>("GetDisplayBpms")?
        .map(|function| call_table_function(table, &function))
        .transpose()?
        .and_then(read_bpms_table)
    {
        if bpms[0] > 0.0 && bpms[1] > 0.0 {
            return Ok(bpms);
        }
    }
    let Some(timing) = table
        .get::<Option<Function>>("GetTimingData")?
        .map(|function| call_table_function(table, &function))
        .transpose()?
        .and_then(|value| match value {
            Value::Table(table) => Some(table),
            _ => None,
        })
    else {
        return Ok([0.0, 0.0]);
    };
    Ok(timing
        .get::<Option<Function>>("GetActualBPM")?
        .map(|function| call_table_function(&timing, &function))
        .transpose()?
        .and_then(read_bpms_table)
        .unwrap_or([0.0, 0.0]))
}

fn call_table_function(table: &Table, function: &Function) -> mlua::Result<Value> {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    function.call(args)
}

fn read_bpms_table(value: Value) -> Option<[f32; 2]> {
    let Value::Table(table) = value else {
        return None;
    };
    Some([
        table.raw_get::<Value>(1).ok().and_then(read_f32)?,
        table.raw_get::<Value>(2).ok().and_then(read_f32)?,
    ])
}

fn display_bpms_text(bpms: [f32; 2], rate: f32) -> String {
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

fn create_radar_values_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetValue",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(table)
}

fn set_string_method(lua: &Lua, table: &Table, name: &str, value: &str) -> mlua::Result<()> {
    let value = value.to_string();
    table.set(
        name,
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&value)?))
        })?,
    )
}

fn set_path_methods(
    lua: &Lua,
    table: &Table,
    path_name: &str,
    has_name: &str,
    path: Option<&Path>,
) -> mlua::Result<()> {
    let path = path.map(file_path_string).unwrap_or_default();
    set_string_method(lua, table, path_name, &path)?;
    let has_file = !path.is_empty();
    table.set(
        has_name,
        lua.create_function(move |_, _args: MultiValue| Ok(has_file))?,
    )?;
    Ok(())
}

#[inline(always)]
fn difficulty_meter(difficulty: SongLuaDifficulty) -> i32 {
    match difficulty {
        SongLuaDifficulty::Beginner => 1,
        SongLuaDifficulty::Easy => 4,
        SongLuaDifficulty::Medium => 7,
        SongLuaDifficulty::Hard => 10,
        SongLuaDifficulty::Challenge => 12,
        SongLuaDifficulty::Edit => 0,
    }
}

fn create_player_options_table(lua: &Lua, player: SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    install_speedmod_method(lua, &table, "CMod", player.speedmod, SongLuaSpeedMod::C)?;
    install_speedmod_state_method(lua, &table, "CAMod", Value::Nil)?;
    install_speedmod_method(lua, &table, "MMod", player.speedmod, SongLuaSpeedMod::M)?;
    install_speedmod_method(lua, &table, "AMod", player.speedmod, SongLuaSpeedMod::A)?;
    install_speedmod_method(lua, &table, "XMod", player.speedmod, SongLuaSpeedMod::X)?;
    table.set(
        "FromString",
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                if let Some(text) = method_arg(&args, 0).cloned().and_then(read_string) {
                    apply_player_options_string(lua, &table, &text)?;
                }
                Ok(table.clone())
            }
        })?,
    )?;
    for name in ["Mirror", "Left", "Right", "Reverse", "Mini", "Skew", "Tilt"] {
        table.set(name, create_player_option_method(lua, &table, name)?)?;
    }
    for name in ["IsEasierForSongAndSteps", "IsEasierForCourseAndTrail"] {
        table.set(name, lua.create_function(|_, _args: MultiValue| Ok(false))?)?;
    }
    table.set(
        "GetReversePercentForColumn",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| player_option_number(lua, &table, "reverse")
        })?,
    )?;
    table.set(
        "UsingReverse",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| Ok(player_option_number(lua, &table, "reverse")? == 1.0)
        })?,
    )?;
    table.set(
        "GetStepAttacks",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                let enabled = player_option_number(lua, &table, "noattack")? <= 0.0
                    && player_option_number(lua, &table, "randattack")? <= 0.0;
                Ok(i64::from(enabled))
            }
        })?,
    )?;
    table.set(
        "DisableTimingWindow",
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                if let Some(window) = method_arg(&args, 0).cloned().and_then(timing_window_name) {
                    disabled_timing_windows(lua, &table)?.set(window, true)?;
                    note_song_lua_side_effect(lua)?;
                }
                Ok(table.clone())
            }
        })?,
    )?;
    table.set(
        "ResetDisabledTimingWindows",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                table.raw_set("__songlua_disabled_timing_windows", lua.create_table()?)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )?;
    table.set(
        "GetDisabledTimingWindows",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                let disabled = disabled_timing_windows(lua, &table)?;
                let out = lua.create_table()?;
                let mut index = 1_i64;
                for window in SONG_LUA_TIMING_WINDOW_NAMES {
                    if disabled.get::<Option<bool>>(window)?.unwrap_or(false) {
                        out.raw_set(index, window)?;
                        index += 1;
                    }
                }
                Ok(out)
            }
        })?,
    )?;
    table.set("__songlua_noteskin_name", player.noteskin_name.clone())?;
    table.set(
        "NoteSkin",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(Value::Nil);
            };
            if let Some(noteskin_name) = method_arg(&args, 0).cloned().and_then(read_string) {
                owner.raw_set("__songlua_noteskin_name", noteskin_name)?;
                return Ok(Value::Table(owner));
            }
            let noteskin_name = owner
                .raw_get::<Option<String>>("__songlua_noteskin_name")?
                .unwrap_or_else(|| player.noteskin_name.clone());
            Ok(Value::String(lua.create_string(&noteskin_name)?))
        })?,
    )?;
    let mt = lua.create_table()?;
    let fallback_owner = table.clone();
    mt.set(
        "__index",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Function(create_player_option_method(
                lua,
                &fallback_owner,
                &name,
            )?))
        })?,
    )?;
    let _ = table.set_metatable(Some(mt));
    Ok(table)
}

fn disabled_timing_windows(lua: &Lua, owner: &Table) -> mlua::Result<Table> {
    if let Some(table) = owner.raw_get::<Option<Table>>("__songlua_disabled_timing_windows")? {
        return Ok(table);
    }
    let table = lua.create_table()?;
    owner.raw_set("__songlua_disabled_timing_windows", table.clone())?;
    Ok(table)
}

fn timing_window_name(value: Value) -> Option<&'static str> {
    let index = match value {
        Value::Integer(value) => i32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() => Some(value.round() as i32),
        Value::String(text) => text
            .to_str()
            .ok()?
            .chars()
            .rev()
            .find(|ch| ch.is_ascii_digit())
            .and_then(|ch| ch.to_digit(10))
            .map(|value| value as i32),
        _ => None,
    }?;
    (1..=5)
        .contains(&index)
        .then_some(SONG_LUA_TIMING_WINDOW_NAMES[index as usize - 1])
}

fn player_option_number(lua: &Lua, owner: &Table, name: &str) -> mlua::Result<f32> {
    Ok(player_option_state(lua, owner)?
        .get::<Option<f32>>(name)?
        .unwrap_or(0.0))
}

fn create_player_option_method(lua: &Lua, owner: &Table, name: &str) -> mlua::Result<Function> {
    let owner = owner.clone();
    let name = name.to_ascii_lowercase();
    lua.create_function(move |lua, args: MultiValue| {
        let state = player_option_state(lua, &owner)?;
        if let Some(value) = method_arg(&args, 0).cloned() {
            state.set(
                name.as_str(),
                normalize_player_option_value(lua, &name, value)?,
            )?;
            return Ok(Value::Table(owner.clone()));
        }
        Ok(match state.get::<Option<Value>>(name.as_str())? {
            Some(value) => value,
            None => default_player_option_value(lua, &name)?,
        })
    })
}

fn player_option_state(lua: &Lua, owner: &Table) -> mlua::Result<Table> {
    if let Some(state) = owner.raw_get::<Option<Table>>("__songlua_player_option_state")? {
        return Ok(state);
    }
    let state = lua.create_table()?;
    owner.raw_set("__songlua_player_option_state", state.clone())?;
    Ok(state)
}

fn apply_player_options_string(lua: &Lua, owner: &Table, text: &str) -> mlua::Result<()> {
    for option in text.split(',') {
        apply_player_option_token(lua, owner, option)?;
    }
    Ok(())
}

fn apply_player_option_token(lua: &Lua, owner: &Table, raw: &str) -> mlua::Result<()> {
    let text = strip_player_option_prefix(raw);
    if text.is_empty() || apply_player_speed_option(owner, text)? {
        return Ok(());
    }

    let (head, tail) = split_first_word(text);
    let (amount, name) = if !tail.is_empty() {
        parse_player_option_amount(head).map_or((None, text), |amount| (Some(amount), tail))
    } else {
        (None, text)
    };
    let key = normalize_player_option_key(name);
    if key.is_empty() {
        return Ok(());
    }

    let state = player_option_state(lua, owner)?;
    let value = if player_option_uses_bool(key.as_str()) {
        Value::Boolean(amount.map_or(true, |amount| amount != 0.0))
    } else {
        Value::Number(amount.unwrap_or(1.0) as f64)
    };
    state.set(key.as_str(), value)
}

fn strip_player_option_prefix(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim_start();
        let Some(rest) = trimmed.strip_prefix('*') else {
            return trimmed;
        };
        let prefix_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if prefix_len == 0 {
            return trimmed;
        }
        text = &rest[prefix_len..];
    }
}

fn split_first_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    match text.find(char::is_whitespace) {
        Some(index) => (&text[..index], text[index..].trim_start()),
        None => (text, ""),
    }
}

fn parse_player_option_amount(text: &str) -> Option<f32> {
    let text = text.trim();
    let percent = text.ends_with('%');
    let raw = text.trim_end_matches('%');
    let value = raw.parse::<f32>().ok()?;
    Some(if percent { value / 100.0 } else { value })
}

fn normalize_player_option_key(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn apply_player_speed_option(owner: &Table, text: &str) -> mlua::Result<bool> {
    let compact: String = text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    if let Some(value) = compact
        .strip_suffix('x')
        .and_then(|raw| raw.parse::<f32>().ok())
    {
        set_player_speedmod(owner, "xmod", Some(value))?;
        return Ok(true);
    }
    for (prefix, key) in [("ca", "camod"), ("c", "cmod"), ("m", "mmod"), ("a", "amod")] {
        if let Some(value) = compact
            .strip_prefix(prefix)
            .and_then(|raw| raw.parse::<f32>().ok())
            .or_else(|| {
                compact
                    .strip_suffix(prefix)
                    .and_then(|raw| raw.parse::<f32>().ok())
            })
        {
            set_player_speedmod(owner, key, Some(value))?;
            return Ok(true);
        }
    }
    Ok(false)
}

#[inline(always)]
fn normalize_player_option_value(lua: &Lua, name: &str, value: Value) -> mlua::Result<Value> {
    if player_option_uses_bool(name) {
        return Ok(Value::Boolean(read_boolish(value).unwrap_or(false)));
    }
    if player_option_default_string(name).is_some() {
        return Ok(match value {
            Value::String(_) => value,
            _ => default_player_option_value(lua, name)?,
        });
    }
    Ok(Value::Number(read_f32(value).unwrap_or(0.0) as f64))
}

#[inline(always)]
fn default_player_option_value(lua: &Lua, name: &str) -> mlua::Result<Value> {
    if player_option_uses_bool(name) {
        return Ok(Value::Boolean(false));
    }
    if let Some(value) = player_option_default_string(name) {
        return Ok(Value::String(lua.create_string(value)?));
    }
    Ok(Value::Number(0.0))
}

#[inline(always)]
fn player_option_default_string(name: &str) -> Option<&'static str> {
    Some(match name {
        "drainsetting" => "DrainType_Normal",
        "failsetting" => "FailType_Immediate",
        "hidelightsetting" => "HideLightType_NoHideLights",
        "lifesetting" => "LifeType_Bar",
        "mintnstohidenotes" => "TapNoteScore_None",
        "modtimersetting" => "ModTimerType_Default",
        _ => return None,
    })
}

#[inline(always)]
fn player_option_uses_bool(name: &str) -> bool {
    matches!(
        name,
        "attackmines"
            | "backwards"
            | "big"
            | "bmrize"
            | "cosecant"
            | "dizzyholds"
            | "echo"
            | "floored"
            | "holdrolls"
            | "hypershuffle"
            | "left"
            | "little"
            | "lrmirror"
            | "mirror"
            | "mines"
            | "muteonerror"
            | "nohands"
            | "noholds"
            | "nojumps"
            | "nolifts"
            | "nomines"
            | "noquads"
            | "norolls"
            | "nostretch"
            | "nofakes"
            | "overhead"
            | "planted"
            | "quick"
            | "right"
            | "shuffle"
            | "skippy"
            | "softshuffle"
            | "stealthpastreceptors"
            | "stealthtype"
            | "stomp"
            | "supershuffle"
            | "turnnone"
            | "twister"
            | "udmirror"
            | "wide"
            | "zbuffer"
    )
}

fn install_speedmod_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    speedmod: SongLuaSpeedMod,
    ctor: fn(f32) -> SongLuaSpeedMod,
) -> mlua::Result<()> {
    let initial = speedmod_value(speedmod, ctor);
    install_speedmod_state_method(lua, table, name, initial)
}

fn install_speedmod_state_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    initial: Value,
) -> mlua::Result<()> {
    let owner = table.clone();
    let key = name.to_ascii_lowercase();
    let value_key = format!("__songlua_speedmod_{key}");
    table.set(
        name,
        lua.create_function(move |_, args: MultiValue| {
            if let Some(value) = method_arg(&args, 0).cloned() {
                if matches!(value, Value::Nil) {
                    set_player_speedmod(&owner, key.as_str(), None)?;
                } else if let Some(value) = read_f32(value) {
                    set_player_speedmod(&owner, key.as_str(), Some(value))?;
                }
                return Ok(Value::Table(owner.clone()));
            }

            let active = owner.raw_get::<Option<String>>("__songlua_speedmod_active")?;
            if active
                .as_deref()
                .is_some_and(|active| active != key.as_str())
            {
                return Ok(Value::Nil);
            }
            if active.as_deref() == Some(key.as_str()) {
                return Ok(owner
                    .raw_get::<Option<f32>>(value_key.as_str())?
                    .map_or(Value::Nil, |value| Value::Number(value as f64)));
            }
            Ok(initial.clone())
        })?,
    )
}

fn set_player_speedmod(owner: &Table, key: &str, value: Option<f32>) -> mlua::Result<()> {
    let value_key = format!("__songlua_speedmod_{key}");
    if let Some(value) = value {
        owner.raw_set("__songlua_speedmod_active", key)?;
        owner.raw_set(value_key.as_str(), value)?;
    } else {
        owner.raw_set("__songlua_speedmod_active", "none")?;
        owner.raw_set(value_key.as_str(), Value::Nil)?;
    }
    Ok(())
}

fn speedmod_value(speedmod: SongLuaSpeedMod, ctor: fn(f32) -> SongLuaSpeedMod) -> Value {
    match (speedmod, ctor(0.0)) {
        (SongLuaSpeedMod::X(value), SongLuaSpeedMod::X(_))
        | (SongLuaSpeedMod::C(value), SongLuaSpeedMod::C(_))
        | (SongLuaSpeedMod::M(value), SongLuaSpeedMod::M(_))
        | (SongLuaSpeedMod::A(value), SongLuaSpeedMod::A(_)) => Value::Number(value as f64),
        _ => Value::Nil,
    }
}

fn create_song_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let song_dir = song_dir_string(context.song_dir.as_path());
    let steps_by_type = create_steps_by_steps_type_table(
        lua,
        context.song_display_bpms,
        context.song_dir.as_path(),
    )?;
    set_string_method(lua, &table, "GetSongDir", &song_dir)?;
    set_string_method(lua, &table, "GetMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDisplayMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetTranslitMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDisplayFullTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetTranslitFullTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDisplaySubTitle", "")?;
    set_string_method(lua, &table, "GetTranslitSubTitle", "")?;
    set_string_method(lua, &table, "GetDisplayArtist", "")?;
    set_string_method(lua, &table, "GetTranslitArtist", "")?;
    set_string_method(
        lua,
        &table,
        "GetGroupName",
        &song_group_name(&context.song_dir),
    )?;
    let music_path = song_music_path(&context.song_dir);
    let banner_path = song_named_image_path(&context.song_dir, &["banner", "bn"]);
    let background_path = song_named_image_path(&context.song_dir, &["background", "bg"]);
    let jacket_path = song_named_image_path(&context.song_dir, &["jacket", "cover"]);
    let cd_image_path = song_named_image_path(&context.song_dir, &["cdtitle", "cdimage", "disc"]);
    set_path_methods(
        lua,
        &table,
        "GetMusicPath",
        "HasMusic",
        music_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetBannerPath",
        "HasBanner",
        banner_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetBackgroundPath",
        "HasBackground",
        background_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetJacketPath",
        "HasJacket",
        jacket_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetCDImagePath",
        "HasCDImage",
        cd_image_path.as_deref(),
    )?;
    let music_length_seconds = context.music_length_seconds.max(0.0);
    table.set(
        "MusicLengthSeconds",
        lua.create_function(move |_, _args: MultiValue| Ok(music_length_seconds))?,
    )?;
    table.set(
        "GetFirstSecond",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetLastSecond",
        lua.create_function(move |_, _args: MultiValue| Ok(music_length_seconds))?,
    )?;
    table.set(
        "GetFirstBeat",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    let last_beat = music_length_seconds * context.song_display_bpms[1].max(0.0) / 60.0;
    table.set(
        "GetLastBeat",
        lua.create_function(move |_, _args: MultiValue| Ok(last_beat))?,
    )?;
    set_string_method(lua, &table, "GetOrTryAtLeastToGetSimfileAuthor", "")?;
    table.set(
        "GetStageCost",
        lua.create_function(|_, _args: MultiValue| Ok(1.0_f32))?,
    )?;
    let display_bpms = create_display_bpms_table(lua, context.song_display_bpms)?;
    table.set(
        "GetDisplayBpms",
        lua.create_function(move |_, _args: MultiValue| Ok(display_bpms.clone()))?,
    )?;
    let timing = create_timing_table(lua, context.song_display_bpms)?;
    table.set(
        "GetTimingData",
        lua.create_function(move |_, _args: MultiValue| Ok(timing.clone()))?,
    )?;
    let all_steps = steps_by_type.clone();
    table.set(
        "GetAllSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(all_steps.clone()))?,
    )?;
    table.set(
        "HasStepsType",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single))
        })?,
    )?;
    table.set(
        "HasStepsTypeAndDifficulty",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single)
                && method_arg(&args, 1)
                    .cloned()
                    .and_then(song_lua_difficulty_from_value)
                    .is_some())
        })?,
    )?;
    table.set(
        "HasEdits",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single))
        })?,
    )?;
    let one_steps = steps_by_type.clone();
    table.set(
        "GetOneSteps",
        lua.create_function(move |_, args: MultiValue| {
            if !method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single)
            {
                return Ok(Value::Nil);
            }
            let Some(difficulty) = method_arg(&args, 1)
                .cloned()
                .and_then(song_lua_difficulty_from_value)
            else {
                return Ok(Value::Nil);
            };
            Ok(one_steps
                .raw_get::<Option<Table>>(usize::from(difficulty.sort_key()) + 1)?
                .map(Value::Table)
                .unwrap_or(Value::Nil))
        })?,
    )?;
    let steps_by_type_for_get = steps_by_type.clone();
    table.set(
        "GetStepsByStepsType",
        lua.create_function(move |lua, args: MultiValue| {
            if !method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single)
            {
                return Ok(lua.create_table()?);
            }
            Ok(steps_by_type_for_get.clone())
        })?,
    )?;
    Ok(table)
}

fn create_course_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    song: Table,
    trail: Table,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let course_dir = context.song_dir.join("compat-course.crs");
    let course_dir = file_path_string(course_dir.as_path());
    set_string_method(lua, &table, "GetDisplayMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDisplayFullTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetTranslitMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetTranslitFullTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDescription", "")?;
    set_string_method(lua, &table, "GetScripter", "")?;
    set_string_method(lua, &table, "GetCourseDir", &course_dir)?;
    set_string_method(lua, &table, "GetCourseType", "CourseType_Nonstop")?;
    set_string_method(lua, &table, "GetDifficulty", "Difficulty_Medium")?;
    let background_path = song_named_image_path(&context.song_dir, &["background", "bg"]);
    let banner_path = song_named_image_path(&context.song_dir, &["banner", "bn"]);
    set_path_methods(
        lua,
        &table,
        "GetBannerPath",
        "HasBanner",
        banner_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetBackgroundPath",
        "HasBackground",
        background_path.as_deref(),
    )?;
    let entries = create_single_value_array(
        lua,
        create_trail_entry_table(lua, song, trail.raw_get::<Table>("__songlua_steps")?)?,
    )?;
    let all_trails = create_single_value_array(lua, trail.clone())?;
    table.set(
        "GetCourseEntries",
        lua.create_function(move |_, _args: MultiValue| Ok(entries.clone()))?,
    )?;
    table.set(
        "GetAllTrails",
        lua.create_function(move |_, _args: MultiValue| Ok(all_trails.clone()))?,
    )?;
    table.set(
        "GetTrail",
        lua.create_function(move |_, _args: MultiValue| Ok(trail.clone()))?,
    )?;
    table.set(
        "GetEstimatedNumStages",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    for (method, value) in [
        ("AllSongsAreFixed", true),
        ("IsAutogen", false),
        ("IsEndless", false),
        ("IsPlayable", true),
    ] {
        table.set(
            method,
            lua.create_function(move |_, _args: MultiValue| Ok(value))?,
        )?;
    }
    Ok(table)
}

fn create_trail_table(
    lua: &Lua,
    song: Table,
    steps: Table,
    display_bpms: [f32; 2],
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set("__songlua_steps", steps.clone())?;
    let entry = create_trail_entry_table(lua, song, steps)?;
    let entries = create_single_value_array(lua, entry.clone())?;
    table.set(
        "GetTrailEntries",
        lua.create_function({
            let entries = entries.clone();
            move |_, _args: MultiValue| Ok(entries.clone())
        })?,
    )?;
    table.set(
        "GetTrailEntry",
        lua.create_function(move |_, args: MultiValue| {
            let index = method_arg(&args, 0)
                .cloned()
                .and_then(read_i32_value)
                .unwrap_or(0);
            let index = usize::try_from(index.max(0)).unwrap_or(0) + 1;
            Ok(entries
                .raw_get::<Option<Table>>(index)?
                .unwrap_or_else(|| entry.clone()))
        })?,
    )?;
    set_string_method(lua, &table, "GetStepsType", "StepsType_Dance_Single")?;
    set_string_method(lua, &table, "GetDifficulty", "Difficulty_Medium")?;
    table.set(
        "GetMeter",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    let display_bpms = create_display_bpms_table(lua, display_bpms)?;
    table.set(
        "GetDisplayBpms",
        lua.create_function(move |_, _args: MultiValue| Ok(display_bpms.clone()))?,
    )?;
    table.set(
        "GetRadarValues",
        lua.create_function(|lua, _args: MultiValue| create_radar_values_table(lua))?,
    )?;
    Ok(table)
}

fn create_trail_entry_table(lua: &Lua, song: Table, steps: Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetSong",
        lua.create_function(move |_, _args: MultiValue| Ok(song.clone()))?,
    )?;
    table.set(
        "GetSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(steps.clone()))?,
    )?;
    set_string_method(lua, &table, "GetCourseEntryType", "CourseEntryType_Fixed")?;
    set_string_method(lua, &table, "GetNormalModifiers", "")?;
    set_string_method(lua, &table, "GetAttackModifiers", "")?;
    table.set(
        "IsSecret",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(table)
}

fn create_songman_table(
    lua: &Lua,
    current_song: Table,
    current_steps: Table,
    current_course: Table,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
    let songman = lua.create_table()?;
    let current_group = song_group_name(&context.song_dir);
    let current_title = context.main_title.clone();
    let current_dir = song_dir_string(context.song_dir.as_path());
    let current_course_dir = file_path_string(context.song_dir.join("compat-course.crs").as_path());
    let all_songs = create_single_value_array(lua, current_song.clone())?;
    let all_courses = create_single_value_array(lua, current_course.clone())?;
    let groups = create_string_array(lua, &[current_group.as_str()])?;
    let course_groups = create_string_array(lua, &[current_group.as_str()])?;
    let group_songs = create_single_value_array(lua, current_song.clone())?;
    let group_courses = create_single_value_array(lua, current_course.clone())?;
    let group = create_song_group_table(lua)?;
    let group_banner_path = song_named_image_path(&context.song_dir, &["banner", "bn"])
        .as_deref()
        .map(file_path_string)
        .unwrap_or_default();

    songman.set(
        "GetSongFromSteps",
        lua.create_function({
            let current_song = current_song.clone();
            move |_, _args: MultiValue| Ok(current_song.clone())
        })?,
    )?;
    songman.set(
        "FindSong",
        lua.create_function({
            let current_song = current_song.clone();
            let current_dir = current_dir.clone();
            let current_group = current_group.clone();
            let current_title = current_title.clone();
            move |_, args: MultiValue| {
                let Some(query) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                if song_lookup_matches(&query, &current_dir, &current_group, &current_title) {
                    Ok(Value::Table(current_song.clone()))
                } else {
                    Ok(Value::Nil)
                }
            }
        })?,
    )?;
    songman.set(
        "FindCourse",
        lua.create_function({
            let current_course = current_course.clone();
            let current_course_dir = current_course_dir.clone();
            let current_dir = current_dir.clone();
            let current_group = current_group.clone();
            let current_title = current_title.clone();
            move |_, args: MultiValue| {
                let Some(query) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                if song_lookup_matches(&query, &current_course_dir, &current_group, &current_title)
                    || song_lookup_matches(&query, &current_dir, &current_group, &current_title)
                {
                    Ok(Value::Table(current_course.clone()))
                } else {
                    Ok(Value::Nil)
                }
            }
        })?,
    )?;
    songman.set(
        "GetRandomSong",
        lua.create_function({
            let current_song = current_song.clone();
            move |_, _args: MultiValue| Ok(current_song.clone())
        })?,
    )?;
    songman.set(
        "GetRandomCourse",
        lua.create_function({
            let current_course = current_course.clone();
            move |_, _args: MultiValue| Ok(current_course.clone())
        })?,
    )?;
    songman.set(
        "GetAllSongs",
        lua.create_function(move |_, _args: MultiValue| Ok(all_songs.clone()))?,
    )?;
    songman.set(
        "GetAllCourses",
        lua.create_function(move |_, _args: MultiValue| Ok(all_courses.clone()))?,
    )?;
    songman.set(
        "GetSongGroupNames",
        lua.create_function(move |_, _args: MultiValue| Ok(groups.clone()))?,
    )?;
    songman.set(
        "GetCourseGroupNames",
        lua.create_function(move |_, _args: MultiValue| Ok(course_groups.clone()))?,
    )?;
    songman.set(
        "GetSongsInGroup",
        lua.create_function({
            let current_group = current_group.clone();
            let group_songs = group_songs.clone();
            move |lua, args: MultiValue| {
                let group = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                if group == current_group {
                    Ok(group_songs.clone())
                } else {
                    lua.create_table()
                }
            }
        })?,
    )?;
    songman.set(
        "GetCoursesInGroup",
        lua.create_function({
            let current_group = current_group.clone();
            let group_courses = group_courses.clone();
            move |lua, args: MultiValue| {
                let group = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                if group == current_group {
                    Ok(group_courses.clone())
                } else {
                    lua.create_table()
                }
            }
        })?,
    )?;
    for method in ["DoesSongGroupExist", "DoesCourseGroupExist"] {
        songman.set(
            method,
            lua.create_function({
                let current_group = current_group.clone();
                move |_, args: MultiValue| {
                    Ok(method_arg(&args, 0)
                        .cloned()
                        .and_then(read_string)
                        .is_some_and(|group| group == current_group))
                }
            })?,
        )?;
    }
    songman.set(
        "GetExtraStageInfo",
        lua.create_function({
            let current_song = current_song.clone();
            let current_steps = current_steps.clone();
            move |_, _args: MultiValue| Ok((current_song.clone(), current_steps.clone()))
        })?,
    )?;
    for method in ["GetSongColor", "GetSongGroupColor", "GetCourseColor"] {
        songman.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                make_color_table(lua, [1.0, 1.0, 1.0, 1.0])
            })?,
        )?;
    }
    songman.set(
        "GetSongRank",
        lua.create_function(|_, args: MultiValue| {
            if matches!(method_arg(&args, 0), Some(Value::Table(_))) {
                Ok(Value::Integer(1))
            } else {
                Ok(Value::Nil)
            }
        })?,
    )?;
    songman.set(
        "ShortenGroupName",
        lua.create_function(|lua, args: MultiValue| {
            let group = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(lua.create_string(&group)?))
        })?,
    )?;
    for method in ["GetSongGroupBannerPath", "GetCourseGroupBannerPath"] {
        songman.set(
            method,
            lua.create_function({
                let current_group = current_group.clone();
                let group_banner_path = group_banner_path.clone();
                move |lua, args: MultiValue| {
                    let group = method_arg(&args, 0)
                        .cloned()
                        .and_then(read_string)
                        .unwrap_or_default();
                    let path = if group == current_group {
                        group_banner_path.as_str()
                    } else {
                        ""
                    };
                    Ok(Value::String(lua.create_string(path)?))
                }
            })?,
        )?;
    }
    songman.set(
        "SongToPreferredSortSectionName",
        lua.create_function({
            let current_group = current_group.clone();
            move |_, args: MultiValue| {
                if matches!(method_arg(&args, 0), Some(Value::Table(_))) {
                    Ok(current_group.clone())
                } else {
                    Ok(String::new())
                }
            }
        })?,
    )?;
    songman.set(
        "GetPreferredSortSongsBySectionName",
        lua.create_function({
            let current_group = current_group.clone();
            let group_songs = group_songs.clone();
            move |lua, args: MultiValue| {
                let section = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                if section == current_group {
                    Ok(group_songs.clone())
                } else {
                    lua.create_table()
                }
            }
        })?,
    )?;
    songman.set(
        "GetGroup",
        lua.create_function(move |_, _args: MultiValue| Ok(group.clone()))?,
    )?;
    for (method, value) in [
        ("GetNumSongs", 1_i64),
        ("GetNumLockedSongs", 0),
        ("GetNumUnlockedSongs", 1),
        ("GetNumSelectableAndUnlockedSongs", 1),
        ("GetNumAdditionalSongs", 0),
        ("GetNumSongGroups", 1),
        ("GetNumCourses", 1),
        ("GetNumAdditionalCourses", 0),
        ("GetNumCourseGroups", 1),
    ] {
        songman.set(
            method,
            lua.create_function(move |_, _args: MultiValue| Ok(value))?,
        )?;
    }
    for method in ["SetPreferredSongs", "SetPreferredCourses"] {
        songman.set(
            method,
            lua.create_function({
                let songman = songman.clone();
                move |lua, _args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    Ok(songman.clone())
                }
            })?,
        )?;
    }
    let preferred_sort_songs = group_songs.clone();
    songman.set(
        "GetPreferredSortSongs",
        lua.create_function(move |_, _args: MultiValue| Ok(preferred_sort_songs.clone()))?,
    )?;
    let preferred_sort_courses = group_courses.clone();
    songman.set(
        "GetPreferredSortCourses",
        lua.create_function(move |_, _args: MultiValue| Ok(preferred_sort_courses.clone()))?,
    )?;
    songman.set(
        "GetPopularSongs",
        lua.create_function(move |_, _args: MultiValue| Ok(group_songs.clone()))?,
    )?;
    let popular_courses = group_courses.clone();
    songman.set(
        "GetPopularCourses",
        lua.create_function(move |_, _args: MultiValue| Ok(popular_courses.clone()))?,
    )?;
    for method in [
        "WasLoadedFromAdditionalSongs",
        "WasLoadedFromAdditionalCourses",
    ] {
        songman.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(false))?,
        )?;
    }
    Ok(songman)
}

fn create_song_group_table(lua: &Lua) -> mlua::Result<Table> {
    let group = lua.create_table()?;
    group.set(
        "GetSyncOffset",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(group)
}

fn create_single_value_array(lua: &Lua, value: Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, value)?;
    Ok(table)
}

fn song_lookup_matches(query: &str, song_dir: &str, group: &str, title: &str) -> bool {
    let query = query.trim().replace('\\', "/");
    !query.is_empty()
        && (query == song_dir
            || song_dir.contains(query.as_str())
            || query.eq_ignore_ascii_case(group)
            || query.eq_ignore_ascii_case(title))
}

fn create_steps_by_steps_type_table(
    lua: &Lua,
    display_bpms: [f32; 2],
    song_dir: &Path,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
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
        table.raw_set(
            idx + 1,
            create_steps_table(lua, difficulty, display_bpms, song_dir)?,
        )?;
    }
    Ok(table)
}

fn create_timing_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let actual_bpms = create_display_bpms_table(lua, bpms)?;
    table.set(
        "GetActualBPM",
        lua.create_function(move |_, _args: MultiValue| Ok(actual_bpms.clone()))?,
    )?;
    let timing_bpms = create_timing_bpms_table(lua, bpms)?;
    table.set(
        "GetBPMs",
        lua.create_function(move |_, _args: MultiValue| Ok(timing_bpms.clone()))?,
    )?;
    let has_bpm_changes = (bpms[0] - bpms[1]).abs() > f32::EPSILON;
    table.set(
        "HasBPMChanges",
        lua.create_function(move |_, _args: MultiValue| Ok(has_bpm_changes))?,
    )?;
    let bpm_at_beat = bpms[0].max(0.0);
    table.set(
        "GetBPMAtBeat",
        lua.create_function(move |_, _args: MultiValue| Ok(bpm_at_beat))?,
    )?;
    table.set(
        "GetElapsedTimeFromBeat",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0))
        })?,
    )?;
    table.set(
        "GetBeatFromElapsedTime",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0))
        })?,
    )?;
    Ok(table)
}

fn create_timing_bpms_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, bpms[0].max(0.0))?;
    if (bpms[0] - bpms[1]).abs() > f32::EPSILON {
        table.raw_set(2, bpms[1].max(0.0))?;
    }
    Ok(table)
}

fn create_song_runtime_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(SONG_LUA_RUNTIME_BEAT_KEY, 0_i64)?;
    table.set(SONG_LUA_RUNTIME_SECONDS_KEY, 0_i64)?;
    table.set(SONG_LUA_RUNTIME_DELTA_BEAT_KEY, 0_i64)?;
    table.set(SONG_LUA_RUNTIME_DELTA_SECONDS_KEY, 0_i64)?;
    table.set(SONG_LUA_RUNTIME_BPS_KEY, song_display_bps(context))?;
    table.set(SONG_LUA_RUNTIME_RATE_KEY, song_music_rate(context))?;
    Ok(table)
}

fn create_song_position_table(lua: &Lua, song_runtime: &Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for method in ["GetSongBeat", "GetSongBeatVisible"] {
        table.set(
            method,
            lua.create_function({
                let song_runtime = song_runtime.clone();
                move |_, _args: MultiValue| {
                    song_lua_runtime_number(song_runtime.get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)?)
                }
            })?,
        )?;
    }
    for method in ["GetMusicSeconds", "GetMusicSecondsVisible"] {
        table.set(
            method,
            lua.create_function({
                let song_runtime = song_runtime.clone();
                move |_, _args: MultiValue| {
                    song_lua_runtime_number(song_runtime.get::<f32>(SONG_LUA_RUNTIME_SECONDS_KEY)?)
                }
            })?,
        )?;
    }
    table.set(
        "GetCurBPS",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| Ok(song_runtime.get::<f32>(SONG_LUA_RUNTIME_BPS_KEY)?)
        })?,
    )?;
    Ok(table)
}

fn song_lua_runtime_number(value: f32) -> mlua::Result<Value> {
    if value.is_finite() && value.fract().abs() <= f32::EPSILON {
        Ok(Value::Integer(value as i64))
    } else {
        Ok(Value::Number(value as f64))
    }
}

fn compile_song_runtime_table(lua: &Lua) -> mlua::Result<Table> {
    lua.globals().get(SONG_LUA_RUNTIME_KEY)
}

fn song_lua_side_effect_count(lua: &Lua) -> mlua::Result<i64> {
    Ok(lua
        .globals()
        .get::<Option<i64>>(SONG_LUA_SIDE_EFFECT_COUNT_KEY)?
        .unwrap_or(0))
}

fn note_song_lua_side_effect(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let count = song_lua_side_effect_count(lua)?;
    globals.set(SONG_LUA_SIDE_EFFECT_COUNT_KEY, count.saturating_add(1))
}

fn record_song_lua_broadcast(lua: &Lua, message: &str, has_params: bool) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(broadcasts) = globals.get::<Option<Table>>(SONG_LUA_BROADCASTS_KEY)? else {
        return Ok(());
    };
    let entry = lua.create_table()?;
    entry.set("message", message)?;
    entry.set("has_params", has_params)?;
    broadcasts.raw_set(broadcasts.raw_len() + 1, entry)?;
    Ok(())
}

fn read_song_lua_broadcasts(table: &Table) -> mlua::Result<Vec<(String, bool)>> {
    let mut out = Vec::new();
    for entry in table.sequence_values::<Table>() {
        let entry = entry?;
        let Some(message) = entry.get::<Option<String>>("message")? else {
            continue;
        };
        out.push((
            message,
            entry.get::<Option<bool>>("has_params")?.unwrap_or(false),
        ));
    }
    Ok(out)
}

fn compile_song_runtime_values(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let runtime = compile_song_runtime_table(lua)?;
    Ok((
        runtime.get(SONG_LUA_RUNTIME_BEAT_KEY)?,
        runtime.get(SONG_LUA_RUNTIME_SECONDS_KEY)?,
    ))
}

fn set_compile_song_runtime_values(lua: &Lua, beat: f32, seconds: f32) -> mlua::Result<()> {
    let runtime = compile_song_runtime_table(lua)?;
    runtime.set(SONG_LUA_RUNTIME_BEAT_KEY, beat)?;
    runtime.set(SONG_LUA_RUNTIME_SECONDS_KEY, seconds)?;
    Ok(())
}

fn compile_song_runtime_delta_values(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let runtime = compile_song_runtime_table(lua)?;
    Ok((
        runtime.get(SONG_LUA_RUNTIME_DELTA_BEAT_KEY)?,
        runtime.get(SONG_LUA_RUNTIME_DELTA_SECONDS_KEY)?,
    ))
}

fn set_compile_song_runtime_delta_values(
    lua: &Lua,
    delta_beat: f32,
    delta_seconds: f32,
) -> mlua::Result<()> {
    let runtime = compile_song_runtime_table(lua)?;
    runtime.set(SONG_LUA_RUNTIME_DELTA_BEAT_KEY, delta_beat)?;
    runtime.set(SONG_LUA_RUNTIME_DELTA_SECONDS_KEY, delta_seconds)?;
    Ok(())
}

fn set_compile_song_runtime_beat(lua: &Lua, beat: f32) -> mlua::Result<()> {
    let runtime = compile_song_runtime_table(lua)?;
    let song_bps = runtime
        .get::<Option<f32>>(SONG_LUA_RUNTIME_BPS_KEY)?
        .unwrap_or(1.0);
    let music_rate = runtime
        .get::<Option<f32>>(SONG_LUA_RUNTIME_RATE_KEY)?
        .unwrap_or(1.0);
    set_compile_song_runtime_values(
        lua,
        beat,
        song_elapsed_seconds_for_beat(beat, song_bps, music_rate),
    )
}

fn create_style_table(lua: &Lua, style_name: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let style_name = style_name.to_string();
    let style_name_for_get = style_name.clone();
    table.set(
        "GetName",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&style_name_for_get)?))
        })?,
    )?;
    set_string_method(lua, &table, "GetStepsType", "StepsType_Dance_Single")?;
    set_string_method(lua, &table, "GetStyleType", "StyleType_OnePlayerOneSide")?;
    table.set(
        "ColumnsPerPlayer",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_NOTE_COLUMNS as i64))?,
    )?;
    table.set(
        "GetWidth",
        lua.create_function(|_, _args: MultiValue| Ok(256.0_f32))?,
    )?;
    table.set(
        "GetColumnInfo",
        lua.create_function(|lua, args: MultiValue| {
            let index = method_arg(&args, 1)
                .cloned()
                .and_then(read_i32_value)
                .unwrap_or(1)
                .clamp(1, SONG_LUA_NOTE_COLUMNS as i32) as usize
                - 1;
            let names = ["Left", "Down", "Up", "Right"];
            let info = lua.create_table()?;
            info.set("Name", names[index])?;
            info.set("Track", index as i64)?;
            info.set("XOffset", SONG_LUA_COLUMN_X[index])?;
            Ok(info)
        })?,
    )?;
    Ok(table)
}

fn install_def(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let globals = lua.globals();
    let def = lua.create_table()?;
    let human_player_count = song_lua_human_player_count(context);
    for &(name, actor_type) in &[
        ("Actor", "Actor"),
        ("ActorFrame", "ActorFrame"),
        ("Sprite", "Sprite"),
        ("Banner", "Sprite"),
        ("ActorMultiVertex", "ActorMultiVertex"),
        ("Sound", "Sound"),
        ("BitmapText", "BitmapText"),
        ("RollingNumbers", "RollingNumbers"),
        ("GraphDisplay", "GraphDisplay"),
        ("SongMeterDisplay", "SongMeterDisplay"),
        ("CourseContentsList", "CourseContentsList"),
        ("DeviceList", "DeviceList"),
        ("InputList", "InputList"),
        ("Model", "Model"),
        ("Quad", "Quad"),
        ("ActorProxy", "ActorProxy"),
        ("ActorFrameTexture", "ActorFrameTexture"),
    ] {
        def.set(name, make_actor_ctor(lua, actor_type, human_player_count)?)?;
    }
    globals.set("Def", def)?;
    globals.set("ActorFrame", create_actorframe_class_table(lua)?)?;
    globals.set("Sprite", create_sprite_class_table(lua)?)?;
    globals.set(
        "LoadFont",
        lua.create_function(|lua, args: MultiValue| {
            let font = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let actor = create_dummy_actor(lua, "BitmapText")?;
            actor.set("Font", font)?;
            Ok(actor)
        })?,
    )?;
    Ok(())
}

fn create_sprite_class_table(lua: &Lua) -> mlua::Result<Table> {
    let class = lua.create_table()?;
    for method_name in [
        "Load",
        "LoadBanner",
        "LoadBackground",
        "LoadFromCached",
        "LoadFromCachedBanner",
        "LoadFromCachedBackground",
        "LoadFromCachedJacket",
        "LoadFromSong",
        "LoadFromCourse",
        "LoadFromSongBackground",
        "LoadFromSongGroup",
        "LoadFromSortOrder",
        "LoadIconFromCharacter",
        "LoadCardFromCharacter",
        "LoadBannerFromUnlockEntry",
        "LoadBackgroundFromUnlockEntry",
        "SetScrolling",
        "GetScrolling",
        "GetPercentScrolling",
        "GetState",
        "SetStateProperties",
        "GetAnimationLengthSeconds",
        "SetSecondsIntoAnimation",
        "SetEffectMode",
        "position",
    ] {
        set_actor_class_forwarder(lua, &class, method_name)?;
    }
    Ok(class)
}

fn set_actor_class_forwarder(
    lua: &Lua,
    class: &Table,
    method_name: &'static str,
) -> mlua::Result<()> {
    class.set(
        method_name,
        lua.create_function(move |_, args: MultiValue| {
            let Some(Value::Table(actor)) = args.front() else {
                return Ok(Value::Nil);
            };
            match actor.get::<Value>(method_name)? {
                Value::Function(method) => method.call::<Value>(args),
                _ => Ok(Value::Nil),
            }
        })?,
    )
}

fn create_actorframe_class_table(lua: &Lua) -> mlua::Result<Table> {
    let class = lua.create_table()?;
    for method_name in [
        "playcommandonchildren",
        "playcommandonleaves",
        "runcommandsonleaves",
        "RunCommandsOnChildren",
        "propagate",
        "fov",
        "SetUpdateRate",
        "GetUpdateRate",
        "SetFOV",
        "vanishpoint",
        "GetChild",
        "GetChildAt",
        "GetChildren",
        "GetNumChildren",
        "SetDrawByZPosition",
        "SetDrawFunction",
        "GetDrawFunction",
        "SetUpdateFunction",
        "SortByDrawOrder",
        "SetAmbientLightColor",
        "SetDiffuseLightColor",
        "SetSpecularLightColor",
        "SetLightDirection",
        "AddChildFromPath",
        "RemoveChild",
        "RemoveAllChildren",
    ] {
        set_actor_class_forwarder(lua, &class, method_name)?;
    }
    class.set(
        "fardistz",
        lua.create_function(|_, args: MultiValue| {
            let Some(actor) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(Value::Nil);
            };
            let Value::Function(method) = actor.get::<Value>("fardistz")? else {
                return Ok(Value::Nil);
            };
            let _ = method.call::<Value>(args)?;
            Ok(Value::Table(actor))
        })?,
    )?;
    Ok(class)
}

fn install_graph_display_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let body_group = lua.create_table()?;
    body_group.set("__songlua_child_group", true)?;
    for index in 1..=2 {
        let child = create_dummy_actor(lua, "GraphDisplayBody")?;
        child.set("__songlua_parent", actor.clone())?;
        body_group.raw_set(index, child)?;
    }
    let line = create_dummy_actor(lua, "GraphDisplayLine")?;
    line.set("Name", "Line")?;
    line.set("__songlua_parent", actor.clone())?;
    let children = actor_children(lua, actor)?;
    children.set("", body_group)?;
    children.set("Line", line)?;
    Ok(())
}

fn capture_graph_display_values(
    lua: &Lua,
    actor: &Table,
    stage_stats: Option<Value>,
    player_stats: Option<Value>,
) -> mlua::Result<()> {
    let total_seconds = stage_stats
        .and_then(|value| match value {
            Value::Table(table) => table
                .get::<Option<f32>>("__songlua_total_steps_seconds")
                .ok()
                .flatten(),
            _ => None,
        })
        .unwrap_or(0.0);
    let values = player_stats
        .and_then(|value| match value {
            Value::Table(table) => {
                let method = table
                    .get::<Option<Function>>("GetLifeRecord")
                    .ok()
                    .flatten()?;
                method
                    .call::<Value>((table, total_seconds, GRAPH_DISPLAY_VALUE_RESOLUTION as i64))
                    .ok()
            }
            _ => None,
        })
        .and_then(|value| match value {
            Value::Table(table) => Some(table),
            _ => None,
        })
        .unwrap_or(create_life_record_table(
            lua,
            GRAPH_DISPLAY_VALUE_RESOLUTION,
            SONG_LUA_INITIAL_LIFE,
        )?);
    actor.set("__songlua_graph_display_values", values)?;
    Ok(())
}

fn install_song_meter_display_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let width = actor
        .get::<Option<Value>>("StreamWidth")?
        .and_then(read_f32)
        .unwrap_or(0.0);
    actor.set("__songlua_stream_width", width)?;
    if let Some(stream) = actor.get::<Option<Table>>("Stream")? {
        if stream.get::<Option<String>>("Name")?.is_none() {
            stream.set("Name", "Stream")?;
        }
        stream.set("__songlua_parent", actor.clone())?;
        actor_children(lua, actor)?.set("Stream", stream)?;
    }
    Ok(())
}

fn install_course_contents_list_children(actor: &Table) -> mlua::Result<()> {
    actor.set("__songlua_scroller_current_item", 0.0_f32)?;
    actor.set("__songlua_scroller_destination_item", 0.0_f32)?;
    actor.set("__songlua_scroller_num_items", 1_i64)?;
    if let Some(display) = actor.get::<Option<Table>>("Display")? {
        if display.get::<Option<String>>("Name")?.is_none() {
            display.set("Name", "Display")?;
        }
        display.set("__songlua_parent", actor.clone())?;
        push_sequence_child_once(actor, display)?;
    }
    Ok(())
}

fn position_scroller_items(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(display) = actor_named_children(lua, actor)?.get::<Option<Table>>("Display")? else {
        return Ok(());
    };
    let num_items = actor
        .get::<Option<i64>>("__songlua_scroller_num_items")?
        .unwrap_or(1)
        .max(1);
    if let Some(transform) =
        actor.get::<Option<Function>>("__songlua_scroller_transform_function")?
    {
        transform.call::<()>((display, 0.0_f32, 0_i64, num_items))?;
    } else if let Some(height) = actor.get::<Option<f32>>("__songlua_scroller_item_height")? {
        display.set("Y", 0.0_f32 * height)?;
    } else if let Some(width) = actor.get::<Option<f32>>("__songlua_scroller_item_width")? {
        display.set("X", 0.0_f32 * width)?;
    }
    Ok(())
}

fn push_sequence_child_once(actor: &Table, child: Table) -> mlua::Result<bool> {
    let child_ptr = child.to_pointer();
    for index in 1..=actor.raw_len() {
        if let Some(Value::Table(existing)) = actor.raw_get::<Option<Value>>(index)?
            && existing.to_pointer() == child_ptr
        {
            return Ok(false);
        }
    }
    actor.raw_set(actor.raw_len() + 1, child)?;
    Ok(true)
}

fn populate_course_contents_display(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(display) = actor_named_children(lua, actor)?.get::<Option<Table>>("Display")? else {
        return Ok(());
    };
    for player_index in 0..LUA_PLAYERS {
        let params = create_course_contents_params(lua, player_index)?;
        let params = Some(Value::Table(params));
        run_actor_named_command_with_drain_and_params(
            lua,
            &display,
            "SetSongCommand",
            true,
            params.clone(),
        )?;
        run_named_command_on_leaves(lua, &display, "SetSongCommand", params)?;
    }
    Ok(())
}

fn create_course_contents_params(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let params = lua.create_table()?;
    params.set("Number", 0_i64)?;
    params.set("PlayerNumber", player_number_name(player_index))?;
    params.set("Song", current_song_value(lua)?)?;
    let steps = current_steps_value(lua, player_index)?;
    params.set("Steps", steps.clone())?;
    set_course_contents_steps_params(lua, &params, steps)?;
    Ok(params)
}

fn current_song_value(lua: &Lua) -> mlua::Result<Value> {
    current_gamestate_value(lua, "GetCurrentSong")
}

fn current_gamestate_value(lua: &Lua, method_name: &str) -> mlua::Result<Value> {
    let Some(gamestate) = lua.globals().get::<Option<Table>>("GAMESTATE")? else {
        return Ok(Value::Nil);
    };
    let Some(method) = gamestate.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>(gamestate)
}

fn current_steps_value(lua: &Lua, player_index: usize) -> mlua::Result<Value> {
    current_gamestate_player_value(lua, "GetCurrentSteps", player_index)
}

fn current_gamestate_player_value(
    lua: &Lua,
    method_name: &str,
    player_index: usize,
) -> mlua::Result<Value> {
    let Some(gamestate) = lua.globals().get::<Option<Table>>("GAMESTATE")? else {
        return Ok(Value::Nil);
    };
    let Some(method) = gamestate.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>((gamestate, player_number_name(player_index)))
}

fn set_course_contents_steps_params(lua: &Lua, params: &Table, steps: Value) -> mlua::Result<()> {
    let Value::Table(steps) = steps else {
        params.set("Meter", "?")?;
        params.set("Difficulty", SongLuaDifficulty::Medium.sm_name())?;
        return Ok(());
    };
    let meter = call_table_method(&steps, "GetMeter")?;
    params.set(
        "Meter",
        if matches!(meter, Value::Nil) {
            Value::String(lua.create_string("?")?)
        } else {
            meter
        },
    )?;
    let difficulty = call_table_method(&steps, "GetDifficulty")?;
    params.set(
        "Difficulty",
        if matches!(difficulty, Value::Nil) {
            Value::String(lua.create_string(SongLuaDifficulty::Medium.sm_name())?)
        } else {
            difficulty
        },
    )
}

fn call_table_method(table: &Table, method_name: &str) -> mlua::Result<Value> {
    let Some(method) = table.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>(table.clone())
}

fn input_status_actor_text(actor_type: &str) -> Option<&'static str> {
    if actor_type.eq_ignore_ascii_case("DeviceList") {
        Some("No input devices")
    } else if actor_type.eq_ignore_ascii_case("InputList") {
        Some("No unmapped inputs")
    } else {
        None
    }
}

fn make_actor_ctor(
    lua: &Lua,
    actor_type: &'static str,
    human_player_count: usize,
) -> mlua::Result<Function> {
    lua.create_function(move |lua, value: Value| {
        let table = match value {
            Value::Table(table) => table,
            _ => lua.create_table()?,
        };
        table.set("__songlua_actor_type", actor_type)?;
        if let Some(script_dir) = lua
            .globals()
            .get::<Option<String>>("__songlua_script_dir")?
        {
            table.set("__songlua_script_dir", script_dir)?;
        }
        if let Some(song_dir) = lua.globals().get::<Option<String>>("__songlua_song_dir")? {
            table.set("__songlua_song_dir", song_dir)?;
        }
        set_actor_decode_movie_for_texture(&table)?;
        install_actor_methods(lua, &table)?;
        install_actor_metatable(lua, &table)?;
        reset_actor_capture(lua, &table)?;
        register_song_lua_actor(lua, &table)?;
        if actor_type.eq_ignore_ascii_case("GraphDisplay") {
            let [width, height] = graph_display_body_size(human_player_count);
            let size = lua.create_table()?;
            size.raw_set(1, width)?;
            size.raw_set(2, height)?;
            table.set("__songlua_state_size", size)?;
            install_graph_display_children(lua, &table)?;
        }
        if actor_type.eq_ignore_ascii_case("SongMeterDisplay") {
            install_song_meter_display_children(lua, &table)?;
        }
        if actor_type.eq_ignore_ascii_case("CourseContentsList") {
            install_course_contents_list_children(&table)?;
        }
        if let Some(text) = input_status_actor_text(actor_type) {
            table.set("Text", text)?;
        }
        Ok(table)
    })
}

fn install_file_loaders(lua: &Lua, song_dir: PathBuf) -> mlua::Result<()> {
    let globals = lua.globals();
    let song_dir_for_loadfile = song_dir.clone();
    globals.set(
        "loadfile",
        lua.create_function(move |lua, path: String| {
            let mut out = MultiValue::new();
            match create_loader_function(lua, &song_dir_for_loadfile, &path) {
                Ok(loader) => {
                    out.push_back(Value::Function(loader));
                    out.push_back(Value::Nil);
                }
                Err(err) => {
                    out.push_back(Value::Nil);
                    out.push_back(Value::String(lua.create_string(err.to_string())?));
                }
            }
            Ok(out)
        })?,
    )?;
    let song_dir_for_dofile = song_dir.clone();
    globals.set(
        "dofile",
        lua.create_function(move |lua, path: String| {
            let loader = create_loader_function(lua, &song_dir_for_dofile, &path)?;
            loader.call::<Value>(())
        })?,
    )?;
    globals.set(
        "LoadActor",
        lua.create_function(move |lua, value: Value| match value {
            Value::String(path) => {
                let path = path.to_str()?.to_string();
                match load_actor_path(lua, &song_dir, &path)? {
                    Value::Table(table) => Ok(table),
                    _ => create_dummy_actor(lua, "LoadActor"),
                }
            }
            _ => create_dummy_actor(lua, "LoadActor"),
        })?,
    )?;
    Ok(())
}

fn load_actor_path(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<Value> {
    if let Some(actor) = create_theme_path_actor(lua, path)? {
        return Ok(Value::Table(actor));
    }
    let resolved = resolve_load_actor_path(lua, song_dir, path)?;
    if is_song_lua_media_path(&resolved) {
        let actor_type = if is_song_lua_audio_path(&resolved) {
            "Sound"
        } else {
            "Sprite"
        };
        return Ok(Value::Table(create_media_actor(
            lua,
            actor_type,
            path,
            resolved.as_path(),
        )?));
    }
    load_script_file(lua, &resolved, song_dir)?.call::<Value>(())
}

fn create_theme_path_actor(lua: &Lua, path: &str) -> mlua::Result<Option<Table>> {
    let theme_path = path.trim_start_matches('/');
    if !theme_path.starts_with(SONG_LUA_THEME_PATH_PREFIX) {
        return Ok(None);
    }
    let path_ref = Path::new(theme_path);
    let actor_type = if is_song_lua_audio_path(path_ref) {
        "Sound"
    } else if is_song_lua_image_path(path_ref) || is_song_lua_video_path(path_ref) {
        "Sprite"
    } else {
        "Actor"
    };
    let actor = create_dummy_actor(lua, actor_type)?;
    if actor_type.eq_ignore_ascii_case("Sound") {
        actor.set("File", path)?;
    } else if actor_type.eq_ignore_ascii_case("Sprite") {
        actor.set("Texture", path)?;
    }
    Ok(Some(actor))
}

fn resolve_load_actor_path(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<PathBuf> {
    if let Ok(resolved) = resolve_script_path(lua, song_dir, path) {
        if resolved.is_dir() {
            return resolve_load_actor_directory(&resolved, song_dir, path);
        }
        if resolved.is_file() {
            return Ok(resolved);
        }
    }
    resolve_load_actor_with_extensions(lua, song_dir, path)
}

fn resolve_load_actor_directory(dir: &Path, song_dir: &Path, path: &str) -> mlua::Result<PathBuf> {
    for candidate in [dir.join("default.lua"), dir.join("default.xml")] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(mlua::Error::external(format!(
        "script '{}' not found relative to '{}'",
        path,
        song_dir.display()
    )))
}

fn resolve_load_actor_with_extensions(
    lua: &Lua,
    song_dir: &Path,
    path: &str,
) -> mlua::Result<PathBuf> {
    let raw = Path::new(path.trim());
    if raw.extension().is_some() {
        return Err(mlua::Error::external(format!(
            "script '{}' not found relative to '{}'",
            path,
            song_dir.display()
        )));
    }

    const LOAD_ACTOR_EXTENSIONS: &[&str] = &[
        "lua", "xml", "png", "jpg", "jpeg", "gif", "bmp", "webp", "apng", "mp4", "avi", "m4v",
        "mov", "webm", "mkv", "mpg", "mpeg", "ogg", "mp3", "wav", "flac", "opus", "m4a", "aac",
    ];

    for base_dir in load_actor_search_dirs(lua, song_dir)? {
        let base = base_dir.join(path);
        if base.is_dir()
            && let Ok(resolved) = resolve_load_actor_directory(&base, song_dir, path)
        {
            return Ok(resolved);
        }
        for ext in LOAD_ACTOR_EXTENSIONS {
            let candidate = base.with_extension(ext);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    Err(mlua::Error::external(format!(
        "script '{}' not found relative to '{}'",
        path,
        song_dir.display()
    )))
}

fn load_actor_search_dirs(lua: &Lua, song_dir: &Path) -> mlua::Result<Vec<PathBuf>> {
    let globals = lua.globals();
    let mut out = Vec::with_capacity(2);
    if let Some(current_dir) = globals
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        let current_dir = PathBuf::from(current_dir);
        if !out.iter().any(|dir| dir == &current_dir) {
            out.push(current_dir);
        }
    }
    if !out.iter().any(|dir| dir == song_dir) {
        out.push(song_dir.to_path_buf());
    }
    Ok(out)
}

fn create_loader_function(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<Function> {
    let path = path.trim();
    let basename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    if basename.eq_ignore_ascii_case("easing.lua") {
        let ease: Table = lua.globals().get("ease")?;
        return lua.create_function(move |_, _args: MultiValue| Ok(Value::Table(ease.clone())));
    }
    let resolved = resolve_script_path(lua, song_dir, path)?;
    load_script_file(lua, &resolved, song_dir)
}

fn load_script_file(lua: &Lua, path: &Path, song_dir: &Path) -> mlua::Result<Function> {
    let source = fs::read_to_string(path).map_err(mlua::Error::external)?;
    let chunk_env = create_chunk_env_proxy(lua, initial_chunk_environment(lua, path)?)?;
    let chunk = lua
        .load(&source)
        .set_name(path.to_string_lossy().as_ref())
        .set_environment(chunk_env.clone());
    let inner = chunk.into_function()?;
    let script_dir = path.parent().unwrap_or(song_dir).to_path_buf();
    let script_path = file_path_string(path);
    let chunk_env_for_call = chunk_env;
    lua.create_function(move |lua, args: MultiValue| {
        call_with_script_dir(lua, &script_dir, || {
            call_with_script_path(lua, &script_path, || {
                call_with_chunk_env(lua, &chunk_env_for_call, || {
                    inner.call::<Value>(args.clone())
                })
            })
        })
    })
}

fn execute_script_file(lua: &Lua, path: &Path, song_dir: &Path) -> mlua::Result<Value> {
    let loader = load_script_file(lua, path, song_dir)?;
    loader.call::<Value>(())
}

fn run_actor_init_commands(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_init_commands_for_table(lua, root)
}

fn run_actor_startup_commands(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_startup_commands_for_table(lua, root)
}

fn run_actor_update_functions(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_update_functions_for_table(lua, root, 1.0_f64 / 60.0)
}

fn run_actor_draw_functions(lua: &Lua, root: &Value) {
    let Value::Table(root) = root else {
        return;
    };
    if let Err(err) = run_actor_draw_functions_for_table(lua, root) {
        debug!("Skipping song lua draw function capture: {err}");
    }
}

fn read_update_function_actions(
    lua: &Lua,
    root: &Value,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    counter: &mut usize,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    let globals = lua.globals();
    let Some(debug) = globals
        .get::<Option<Table>>("debug")
        .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };
    let Some(getupvalue) = debug
        .get::<Option<Function>>("getupvalue")
        .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };
    let mut seen_tables = HashSet::new();
    read_update_function_actions_for_table(
        lua,
        root,
        &getupvalue,
        overlays,
        tracked_actors,
        messages,
        counter,
        &mut seen_tables,
        info,
    )
}

fn run_actor_init_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor
        .get::<Option<bool>>("__songlua_init_commands_ran")?
        .unwrap_or(false)
    {
        return Ok(());
    }
    run_actor_init_command(lua, actor)?;
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_init_commands_for_table(lua, &child)?;
    }
    run_song_meter_stream_init_command(lua, actor)?;
    actor.set("__songlua_init_commands_ran", true)?;
    Ok(())
}

fn run_actor_startup_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor
        .get::<Option<bool>>("__songlua_startup_commands_ran")?
        .unwrap_or(false)
    {
        return Ok(());
    }
    actor.set("__songlua_startup_command_started", true)?;
    actor.set("__songlua_startup_children_walked", false)?;
    if actor_runs_startup_commands(actor)? {
        run_actor_named_command_with_drain(lua, actor, "OnCommand", false)?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_startup_commands_for_table(lua, &child)?;
    }
    run_song_meter_stream_startup_command(lua, actor)?;
    actor.set("__songlua_startup_children_walked", true)?;
    drain_actor_command_queue(lua, actor)?;
    actor.set("__songlua_startup_commands_ran", true)?;
    Ok(())
}

fn song_meter_stream_child(lua: &Lua, actor: &Table) -> mlua::Result<Option<Table>> {
    if !actor_type_is(actor, "SongMeterDisplay")? {
        return Ok(None);
    }
    let Some(stream) = actor_named_children(lua, actor)?.get::<Option<Table>>("Stream")? else {
        return Ok(None);
    };
    stream.set("__songlua_parent", actor.clone())?;
    Ok(Some(stream))
}

fn run_song_meter_stream_init_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(stream) = song_meter_stream_child(lua, actor)? else {
        return Ok(());
    };
    run_actor_init_commands_for_table(lua, &stream)
}

fn run_song_meter_stream_startup_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(stream) = song_meter_stream_child(lua, actor)? else {
        return Ok(());
    };
    run_actor_startup_commands_for_table(lua, &stream)
}

fn actor_update_rate(actor: &Table) -> mlua::Result<f64> {
    let rate = actor
        .get::<Option<f64>>("__songlua_update_rate")?
        .unwrap_or(1.0);
    Ok(if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    })
}

fn run_actor_update_functions_for_table(
    lua: &Lua,
    actor: &Table,
    parent_delta_seconds: f64,
) -> mlua::Result<()> {
    let delta_seconds = parent_delta_seconds * actor_update_rate(actor)?;
    if let Some(update) = actor.get::<Option<Function>>("__songlua_update_function")? {
        call_actor_function(lua, actor, &update, Some(Value::Number(delta_seconds)))?;
        drain_actor_command_queue(lua, actor)?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_update_functions_for_table(lua, &child, delta_seconds)?;
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        run_actor_update_functions_for_table(lua, &stream, delta_seconds)?;
    }
    Ok(())
}

fn run_actor_draw_functions_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(draw) = actor.get::<Option<Function>>("__songlua_draw_function")? {
        let draw_result = call_actor_function(lua, actor, &draw, None);
        let drain_result = drain_actor_command_queue(lua, actor);
        if let Err(err) = draw_result {
            debug!(
                "Skipping song lua draw capture for {}: {}",
                actor_debug_label(actor),
                err
            );
        }
        if let Err(err) = drain_result {
            debug!(
                "Skipping queued song lua draw commands for {}: {}",
                actor_debug_label(actor),
                err
            );
        }
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_draw_functions_for_table(lua, &child)?;
    }
    Ok(())
}

fn read_update_function_actions_for_table(
    lua: &Lua,
    actor: &Table,
    getupvalue: &Function,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    counter: &mut usize,
    seen_tables: &mut HashSet<usize>,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    if let Some(update) = actor
        .get::<Option<Function>>("__songlua_update_function")
        .map_err(|err| err.to_string())?
    {
        for table in update_function_action_tables(getupvalue, &update, seen_tables)? {
            read_actions(
                lua,
                Some(table),
                overlays,
                tracked_actors,
                messages,
                counter,
                info,
            )?;
        }
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        read_update_function_actions_for_table(
            lua,
            &child,
            getupvalue,
            overlays,
            tracked_actors,
            messages,
            counter,
            seen_tables,
            info,
        )?;
    }
    Ok(())
}

fn update_function_action_tables(
    getupvalue: &Function,
    function: &Function,
    seen_tables: &mut HashSet<usize>,
) -> Result<Vec<Table>, String> {
    let mut out = Vec::new();
    for index in 1..=function.info().num_upvalues {
        let (name, value): (Value, Value) = getupvalue
            .call((function.clone(), i64::from(index)))
            .map_err(|err| err.to_string())?;
        let Value::String(name) = name else {
            continue;
        };
        let name = name.to_str().map_err(|err| err.to_string())?;
        if !matches!(name.as_ref(), "mod_actions" | "actions") {
            continue;
        }
        let Value::Table(table) = value else {
            continue;
        };
        if seen_tables.insert(table.to_pointer() as usize) {
            out.push(table);
        }
    }
    Ok(out)
}

fn run_actor_init_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    run_actor_named_command(lua, actor, "InitCommand")
}

fn run_actor_named_command(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    run_actor_named_command_with_drain(lua, actor, name, true)
}

fn run_actor_named_command_with_drain(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
) -> mlua::Result<()> {
    run_actor_named_command_with_drain_and_params(lua, actor, name, drain_queue, None)
}

fn run_actor_named_command_with_drain_and_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let Some(command) = actor.get::<Option<Function>>(name)? else {
        return Ok(());
    };
    run_guarded_actor_command(lua, actor, name, &command, drain_queue, params)
}

fn actor_runs_startup_commands(actor: &Table) -> mlua::Result<bool> {
    let _ = actor;
    Ok(true)
}

fn actor_command_args(actor: &Table, params: Option<Value>) -> MultiValue {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(actor.clone()));
    if let Some(params) = params {
        args.push_back(params);
    }
    args
}

fn call_actor_function(
    lua: &Lua,
    actor: &Table,
    command: &Function,
    params: Option<Value>,
) -> mlua::Result<()> {
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        return call_with_script_dir(lua, Path::new(&script_dir), || {
            command.call::<()>(actor_command_args(actor, params))
        });
    }
    command.call::<()>(actor_command_args(actor, params))
}

fn run_guarded_actor_command(
    lua: &Lua,
    actor: &Table,
    name: &str,
    command: &Function,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let active = actor_active_commands(lua, actor)?;
    if active.get::<Option<bool>>(name)?.unwrap_or(false) {
        return Ok(());
    }
    active.set(name, true)?;
    let result = call_actor_function(lua, actor, command, params).map_err(|err| {
        mlua::Error::external(format!(
            "{} failed for {}: {err}",
            name,
            actor_debug_label(actor)
        ))
    });
    active.set(name, Value::Nil)?;
    result?;
    if drain_queue {
        drain_actor_command_queue(lua, actor)?;
    }
    Ok(())
}

fn actor_active_commands(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(active) = actor.get::<Option<Table>>("__songlua_active_commands")? {
        return Ok(active);
    }
    let active = lua.create_table()?;
    actor.set("__songlua_active_commands", active.clone())?;
    Ok(active)
}

fn actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(queue) = actor.get::<Option<Table>>("__songlua_command_queue")? {
        return Ok(queue);
    }
    let queue = lua.create_table()?;
    actor.set("__songlua_command_queue", queue.clone())?;
    Ok(queue)
}

fn drain_actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let queue = actor_command_queue(lua, actor)?;
    while queue.raw_len() > 0 {
        let Some(name) = queue.raw_get::<Option<String>>(1)? else {
            break;
        };
        let len = queue.raw_len();
        for index in 1..len {
            let value = queue.raw_get::<Value>(index + 1)?;
            queue.raw_set(index, value)?;
        }
        queue.raw_set(len, Value::Nil)?;
        run_actor_named_command(lua, actor, &format!("{name}Command"))?;
    }
    Ok(())
}

fn actor_debug_label(actor: &Table) -> String {
    let actor_type = actor
        .get::<Option<String>>("__songlua_actor_type")
        .ok()
        .flatten()
        .unwrap_or_else(|| "Actor".to_string());
    let name = actor.get::<Option<String>>("Name").ok().flatten();
    match name {
        Some(name) if !name.trim().is_empty() => format!("{actor_type} '{name}'"),
        _ => actor_type,
    }
}

fn read_overlay_actors(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    info: &mut SongLuaCompileInfo,
) -> Result<Vec<OverlayCompileActor>, String> {
    let Value::Table(root) = root else {
        return Ok(Vec::new());
    };
    let mut aft_capture_names = HashSet::new();
    collect_aft_capture_names(root, &mut aft_capture_names)?;
    let mut out = Vec::new();
    read_overlay_actors_from_table(lua, root, None, &aft_capture_names, &mut out, context, info)?;
    Ok(out)
}

fn read_overlay_actors_from_table(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    aft_capture_names: &HashSet<String>,
    out: &mut Vec<OverlayCompileActor>,
    context: &SongLuaCompileContext,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    let next_parent_index = if let Some(overlay) =
        read_overlay_actor(lua, actor, parent_index, aft_capture_names, context, info)?
    {
        let index = out.len();
        out.push(overlay);
        Some(index)
    } else {
        parent_index
    };
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        read_overlay_actors_from_table(
            lua,
            &child,
            next_parent_index,
            aft_capture_names,
            out,
            context,
            info,
        )?;
    }
    Ok(())
}

fn read_overlay_actor(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    aft_capture_names: &HashSet<String>,
    context: &SongLuaCompileContext,
    info: &mut SongLuaCompileInfo,
) -> Result<Option<OverlayCompileActor>, String> {
    let Some(actor_type) = actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let initial_state = overlay_state_after_blocks(actor_overlay_initial_state(actor)?, &[], 0.0);
    let mut message_commands = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair.map_err(|err| err.to_string())?;
        let Some(name) = read_string(key) else {
            continue;
        };
        if !name.ends_with("MessageCommand") || !matches!(value, Value::Function(_)) {
            continue;
        }
        let message = name.trim_end_matches("MessageCommand").to_string();
        let blocks = match capture_actor_command_preserving_state(lua, actor, name.as_str()) {
            Ok(blocks) => blocks,
            Err(err) => {
                let skipped = format!("{}.{}: {err}", actor_debug_label(actor), name);
                push_unique_compile_detail(
                    &mut info.skipped_message_command_captures,
                    skipped.clone(),
                );
                debug!("Skipping song lua overlay message capture for {}", skipped);
                continue;
            }
        };
        if !blocks.is_empty() {
            message_commands.push(SongLuaOverlayMessageCommand { message, blocks });
        }
    }
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    let startup_sound_blocks: Vec<_> = read_actor_capture_blocks(actor)?
        .into_iter()
        .filter(|block| block.delta.sound_play == Some(true))
        .collect();
    if !startup_sound_blocks.is_empty() {
        message_commands.push(SongLuaOverlayMessageCommand {
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            blocks: startup_sound_blocks,
        });
    }
    let name = actor
        .get::<Option<String>>("Name")
        .map_err(|err| err.to_string())?;

    let kind = if actor_type.eq_ignore_ascii_case("Actor") {
        if name.is_none()
            && initial_state == SongLuaOverlayState::default()
            && message_commands.is_empty()
        {
            return Ok(None);
        }
        SongLuaOverlayKind::Actor
    } else if actor_type.eq_ignore_ascii_case("ActorFrame") {
        let has_draw_function = actor
            .get::<Option<Function>>("__songlua_draw_function")
            .map_err(|err| err.to_string())?
            .is_some();
        if parent_index.is_none()
            && name.is_none()
            && initial_state == SongLuaOverlayState::default()
            && message_commands.is_empty()
            && !has_draw_function
        {
            return Ok(None);
        }
        SongLuaOverlayKind::ActorFrame
    } else if actor_type.eq_ignore_ascii_case("ActorFrameTexture") {
        SongLuaOverlayKind::ActorFrameTexture
    } else if actor_type.eq_ignore_ascii_case("ActorProxy") {
        let Some(target) = read_proxy_target_kind(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::ActorProxy { target }
    } else if actor_type.eq_ignore_ascii_case("Sprite") {
        if let Some(capture_name) = actor
            .get::<Option<String>>("__songlua_aft_capture_name")
            .map_err(|err| err.to_string())?
            .filter(|name| !name.trim().is_empty())
        {
            SongLuaOverlayKind::AftSprite { capture_name }
        } else {
            let Some(texture) = actor
                .get::<Option<String>>("Texture")
                .map_err(|err| err.to_string())?
            else {
                return Ok(None);
            };
            if aft_capture_names.contains(&texture) {
                SongLuaOverlayKind::AftSprite {
                    capture_name: texture,
                }
            } else {
                let Some(texture_path) = resolve_actor_asset_path(actor, &texture).ok() else {
                    return Ok(None);
                };
                SongLuaOverlayKind::Sprite { texture_path }
            }
        }
    } else if actor_type.eq_ignore_ascii_case("Sound") {
        let Some(file) = actor
            .get::<Option<String>>("File")
            .map_err(|err| err.to_string())?
        else {
            return Ok(None);
        };
        let Ok(sound_path) = resolve_actor_asset_path(actor, &file) else {
            return Ok(None);
        };
        SongLuaOverlayKind::Sound { sound_path }
    } else if actor_type.eq_ignore_ascii_case("BitmapText")
        || actor_type.eq_ignore_ascii_case("RollingNumbers")
    {
        let Some((font_name, font_path)) = read_bitmap_font(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::BitmapText {
            font_name,
            font_path,
            text: Arc::<str>::from(
                actor
                    .get::<Option<String>>("Text")
                    .map_err(|err| err.to_string())?
                    .unwrap_or_default(),
            ),
            stroke_color: read_actor_color_field(actor, "__songlua_stroke_color")?
                .or_else(|| read_actor_color_field(actor, "StrokeColor").ok().flatten()),
            attributes: read_bitmap_text_attributes(actor)?,
        }
    } else if actor_type.eq_ignore_ascii_case("DeviceList")
        || actor_type.eq_ignore_ascii_case("InputList")
    {
        let Some((font_name, font_path)) = read_bitmap_font(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::BitmapText {
            font_name,
            font_path,
            text: Arc::<str>::from(input_status_actor_text(&actor_type).unwrap_or_default()),
            stroke_color: None,
            attributes: Arc::<[TextAttribute]>::from([]),
        }
    } else if actor_type.eq_ignore_ascii_case("ActorMultiVertex") {
        let Some(vertices) = read_actor_multi_vertex_mesh(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::ActorMultiVertex {
            vertices,
            texture_path: read_actor_multi_vertex_texture_path(actor, aft_capture_names)?,
        }
    } else if actor_type.eq_ignore_ascii_case("Model") {
        let Some(layers) = read_model_layers(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::Model { layers }
    } else if actor_type.eq_ignore_ascii_case("SongMeterDisplay") {
        let Some((stream_width, stream_state)) = read_song_meter_display_state(lua, actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::SongMeterDisplay {
            stream_width,
            stream_state,
            music_length_seconds: context.music_length_seconds.max(0.0),
        }
    } else if actor_type.eq_ignore_ascii_case("CourseContentsList") {
        SongLuaOverlayKind::ActorFrame
    } else if actor_type.eq_ignore_ascii_case("GraphDisplay") {
        SongLuaOverlayKind::GraphDisplay {
            size: read_graph_display_size(initial_state, context),
            body_values: read_graph_display_values(actor)?,
            body_state: read_graph_display_body_state(lua, actor)?,
            line_state: read_graph_display_line_state(lua, actor)?,
        }
    } else if actor_type.eq_ignore_ascii_case("Quad") {
        SongLuaOverlayKind::Quad
    } else {
        return Ok(None);
    };
    Ok(Some(OverlayCompileActor {
        table: actor.clone(),
        actor: SongLuaOverlayActor {
            kind,
            name,
            parent_index,
            initial_state,
            message_commands,
        },
    }))
}

fn read_actor_multi_vertex_texture_path(
    actor: &Table,
    aft_capture_names: &HashSet<String>,
) -> Result<Option<PathBuf>, String> {
    if actor
        .get::<Option<String>>("__songlua_aft_capture_name")
        .map_err(|err| err.to_string())?
        .is_some()
    {
        return Ok(None);
    }
    let Some(texture) = actor
        .get::<Option<String>>("Texture")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    if aft_capture_names.contains(&texture) {
        return Ok(None);
    }
    Ok(resolve_actor_asset_path(actor, &texture).ok())
}

fn read_model_layers(actor: &Table) -> Result<Option<Arc<[SongLuaOverlayModelLayer]>>, String> {
    let Some(model_path) = read_model_path(actor)? else {
        return Ok(None);
    };
    let slots = crate::game::parsing::noteskin::load_itg_model_slots_from_path(&model_path)?;
    let mut layers = Vec::with_capacity(slots.len());
    for slot in slots.iter() {
        let Some(model) = slot.model.as_ref() else {
            continue;
        };
        if model.vertices.is_empty() {
            continue;
        }
        let uv_rect = slot.uv_for_frame_at(0, 0.0);
        let (uv_scale, uv_offset, uv_tex_shift) = song_lua_model_uv_params(slot, uv_rect);
        layers.push(SongLuaOverlayModelLayer {
            texture_key: slot.texture_key_shared(),
            vertices: crate::game::parsing::noteskin::build_model_geometry(slot),
            model_size: model.size(),
            uv_scale,
            uv_offset,
            uv_tex_shift,
            draw: song_lua_model_draw(slot.model_draw_at(0.0, 0.0)),
        });
    }
    if layers.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::from(layers.into_boxed_slice())))
    }
}

fn read_model_path(actor: &Table) -> Result<Option<PathBuf>, String> {
    for key in ["Meshes", "Materials", "Bones"] {
        let Some(raw) = actor
            .get::<Option<String>>(key)
            .map_err(|err| err.to_string())?
            .filter(|path| !path.trim().is_empty())
        else {
            continue;
        };
        if let Ok(path) = resolve_actor_asset_path(actor, &raw) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn song_lua_model_uv_params(
    slot: &crate::game::parsing::noteskin::SpriteSlot,
    uv_rect: [f32; 4],
) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let uv_scale = [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]];
    let uv_offset = [uv_rect[0], uv_rect[1]];
    let uv_tex_shift = match slot.source.as_ref() {
        crate::game::parsing::noteskin::SpriteSource::Atlas { tex_dims, .. } => {
            let tw = tex_dims.0.max(1) as f32;
            let th = tex_dims.1.max(1) as f32;
            let base_u0 = slot.def.src[0] as f32 / tw;
            let base_v0 = slot.def.src[1] as f32 / th;
            [uv_offset[0] - base_u0, uv_offset[1] - base_v0]
        }
        crate::game::parsing::noteskin::SpriteSource::Animated { .. } => [0.0, 0.0],
    };
    (uv_scale, uv_offset, uv_tex_shift)
}

fn song_lua_model_draw(
    draw: crate::game::parsing::noteskin::ModelDrawState,
) -> SongLuaOverlayModelDraw {
    SongLuaOverlayModelDraw {
        pos: draw.pos,
        rot: draw.rot,
        zoom: draw.zoom,
        tint: draw.tint,
        vert_align: draw.vert_align,
        blend_add: draw.blend_add,
        visible: draw.visible,
    }
}

fn read_graph_display_size(
    state: SongLuaOverlayState,
    context: &SongLuaCompileContext,
) -> [f32; 2] {
    let fallback = graph_display_body_size(song_lua_human_player_count(context));
    let size = state.size.unwrap_or(fallback);
    [size[0].abs().max(1.0), size[1].abs().max(1.0)]
}

fn read_graph_display_line_state(lua: &Lua, actor: &Table) -> Result<SongLuaOverlayState, String> {
    let Some(line) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("Line")
        .map_err(|err| err.to_string())?
    else {
        return Ok(SongLuaOverlayState::default());
    };
    actor_overlay_initial_state(&line)
}

fn read_graph_display_values(actor: &Table) -> Result<Arc<[f32]>, String> {
    let values = actor
        .get::<Option<Table>>("__songlua_graph_display_values")
        .map_err(|err| err.to_string())?;
    let Some(values) = values else {
        return Ok(default_graph_display_values());
    };
    let mut out = Vec::with_capacity(GRAPH_DISPLAY_VALUE_RESOLUTION);
    for index in 1..=GRAPH_DISPLAY_VALUE_RESOLUTION {
        let value = values
            .raw_get::<Option<Value>>(index)
            .map_err(|err| err.to_string())?
            .and_then(read_f32)
            .unwrap_or(SONG_LUA_INITIAL_LIFE)
            .clamp(0.0, 1.0);
        out.push(value);
    }
    Ok(Arc::from(out.into_boxed_slice()))
}

fn default_graph_display_values() -> Arc<[f32]> {
    Arc::from([SONG_LUA_INITIAL_LIFE; GRAPH_DISPLAY_VALUE_RESOLUTION])
}

fn read_graph_display_body_state(lua: &Lua, actor: &Table) -> Result<SongLuaOverlayState, String> {
    let Some(body_group) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("")
        .map_err(|err| err.to_string())?
    else {
        return Ok(SongLuaOverlayState::default());
    };
    let Some(body) = body_group
        .raw_get::<Option<Table>>(2)
        .map_err(|err| err.to_string())?
        .or_else(|| body_group.raw_get::<Option<Table>>(1).ok().flatten())
    else {
        return Ok(SongLuaOverlayState::default());
    };
    actor_overlay_initial_state(&body)
}

fn read_song_meter_display_state(
    lua: &Lua,
    actor: &Table,
) -> Result<Option<(f32, SongLuaOverlayState)>, String> {
    let stream_width = actor
        .get::<Option<f32>>("__songlua_stream_width")
        .map_err(|err| err.to_string())?
        .or_else(|| {
            actor
                .get::<Option<Value>>("StreamWidth")
                .ok()
                .flatten()
                .and_then(read_f32)
        })
        .unwrap_or(0.0)
        .max(0.0);
    if stream_width <= f32::EPSILON {
        return Ok(None);
    }
    let Some(stream) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("Stream")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let stream_state = actor_overlay_initial_state(&stream)?;
    Ok(Some((stream_width, stream_state)))
}

fn read_actor_multi_vertex_mesh(
    actor: &Table,
) -> Result<Option<Arc<[SongLuaOverlayMeshVertex]>>, String> {
    let Some(vertices) = actor
        .get::<Option<Table>>("__songlua_vertices")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let source = read_actor_multi_vertex_points(&vertices)?;
    let Some((start, end)) = read_actor_multi_vertex_range(actor, source.len())? else {
        return Ok(None);
    };
    let draw_mode = actor
        .get::<Option<String>>("__songlua_draw_state_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| "DrawMode_Triangles".to_string());
    let line_width = actor
        .get::<Option<f32>>("__songlua_line_width")
        .map_err(|err| err.to_string())?
        .unwrap_or(1.0)
        .max(0.0);
    let mesh = actor_multi_vertex_triangles(&source[start..end], draw_mode.as_str(), line_width);
    if mesh.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::from(mesh.into_boxed_slice())))
    }
}

fn read_actor_multi_vertex_range(
    actor: &Table,
    vertex_count: usize,
) -> Result<Option<(usize, usize)>, String> {
    if vertex_count == 0 {
        return Ok(None);
    }
    let state = actor
        .get::<Option<Table>>("__songlua_draw_state")
        .map_err(|err| err.to_string())?;
    let first = state
        .as_ref()
        .and_then(|state| state.get::<Option<Value>>("First").ok().flatten())
        .and_then(read_i32_value)
        .unwrap_or(1)
        .max(1) as usize;
    let start = first.saturating_sub(1).min(vertex_count);
    let num = state
        .as_ref()
        .and_then(|state| state.get::<Option<Value>>("Num").ok().flatten())
        .and_then(read_i32_value)
        .unwrap_or(-1);
    let end = if num < 0 {
        vertex_count
    } else {
        start.saturating_add(num as usize).min(vertex_count)
    };
    Ok((end > start).then_some((start, end)))
}

fn read_actor_multi_vertex_points(
    vertices: &Table,
) -> Result<Vec<SongLuaActorMultiVertexPoint>, String> {
    let mut out = Vec::with_capacity(vertices.raw_len());
    for value in vertices.sequence_values::<Value>() {
        let Some(vertex) = read_actor_multi_vertex_point(value.map_err(|err| err.to_string())?)?
        else {
            continue;
        };
        out.push(vertex);
    }
    Ok(out)
}

fn read_actor_multi_vertex_point(
    value: Value,
) -> Result<Option<SongLuaActorMultiVertexPoint>, String> {
    let Value::Table(vertex) = value else {
        return Ok(None);
    };
    let Value::Table(position) = vertex.raw_get::<Value>(1).map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let Some(x) = read_f32(
        position
            .raw_get::<Value>(1)
            .map_err(|err| err.to_string())?,
    ) else {
        return Ok(None);
    };
    let Some(y) = read_f32(
        position
            .raw_get::<Value>(2)
            .map_err(|err| err.to_string())?,
    ) else {
        return Ok(None);
    };
    let color = read_color_value(vertex.raw_get::<Value>(2).map_err(|err| err.to_string())?)
        .unwrap_or([1.0; 4]);
    let uv = match vertex.raw_get::<Value>(3).map_err(|err| err.to_string())? {
        Value::Table(table) => [
            table
                .raw_get::<Value>(1)
                .ok()
                .and_then(read_f32)
                .unwrap_or(0.0),
            table
                .raw_get::<Value>(2)
                .ok()
                .and_then(read_f32)
                .unwrap_or(0.0),
        ],
        _ => [0.0, 0.0],
    };
    Ok(Some(SongLuaActorMultiVertexPoint {
        pos: [x, y],
        color,
        uv,
    }))
}

fn actor_multi_vertex_triangles(
    vertices: &[SongLuaActorMultiVertexPoint],
    draw_mode: &str,
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if draw_mode.eq_ignore_ascii_case("DrawMode_Quads") {
        return actor_multi_vertex_quads(vertices);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_QuadStrip") {
        return actor_multi_vertex_quad_strip(vertices);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_LineStrip") {
        return actor_multi_vertex_line_strip(vertices, line_width);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_Fan") {
        return actor_multi_vertex_fan(vertices);
    }
    actor_multi_vertex_triangle_list(vertices)
}

fn actor_multi_vertex_triangle_list(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len() / 3 * 3);
    for chunk in vertices.chunks_exact(3) {
        push_actor_multi_vertex_triangle(&mut out, chunk[0], chunk[1], chunk[2]);
    }
    out
}

fn actor_multi_vertex_quads(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len() / 4 * 6);
    for chunk in vertices.chunks_exact(4) {
        push_actor_multi_vertex_quad(&mut out, chunk[0], chunk[1], chunk[2], chunk[3]);
    }
    out
}

fn actor_multi_vertex_quad_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(2) / 2 * 6);
    let mut index = 0usize;
    while index + 3 < vertices.len() {
        push_actor_multi_vertex_quad(
            &mut out,
            vertices[index],
            vertices[index + 1],
            vertices[index + 3],
            vertices[index + 2],
        );
        index += 2;
    }
    out
}

fn actor_multi_vertex_fan(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(2) * 3);
    for index in 1..vertices.len() - 1 {
        push_actor_multi_vertex_triangle(
            &mut out,
            vertices[0],
            vertices[index],
            vertices[index + 1],
        );
    }
    out
}

fn actor_multi_vertex_line_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 2 || line_width <= f32::EPSILON {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(1) * 6);
    let half_width = 0.5 * line_width;
    let mut offsets = Vec::with_capacity(vertices.len());
    for index in 0..vertices.len() {
        let offset = if index == 0 {
            actor_multi_vertex_line_normal(vertices[0], vertices[1], half_width)
        } else if index + 1 == vertices.len() {
            actor_multi_vertex_line_normal(vertices[index - 1], vertices[index], half_width)
        } else {
            actor_multi_vertex_line_join_offset(
                vertices[index - 1],
                vertices[index],
                vertices[index + 1],
                half_width,
            )
        };
        offsets.push(offset);
    }
    for index in 0..vertices.len().saturating_sub(1) {
        let a = vertices[index];
        let b = vertices[index + 1];
        if actor_multi_vertex_segment_len(a, b) <= f32::EPSILON {
            continue;
        }
        let a0 = actor_multi_vertex_offset_point(a, offsets[index], 1.0);
        let a1 = actor_multi_vertex_offset_point(a, offsets[index], -1.0);
        let b0 = actor_multi_vertex_offset_point(b, offsets[index + 1], 1.0);
        let b1 = actor_multi_vertex_offset_point(b, offsets[index + 1], -1.0);
        push_actor_multi_vertex_triangle(&mut out, a0, b0, b1);
        push_actor_multi_vertex_triangle(&mut out, a0, b1, a1);
    }
    out
}

fn actor_multi_vertex_segment_len(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
) -> f32 {
    (b.pos[0] - a.pos[0]).hypot(b.pos[1] - a.pos[1])
}

fn actor_multi_vertex_line_normal(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    half_width: f32,
) -> [f32; 2] {
    let dx = b.pos[0] - a.pos[0];
    let dy = b.pos[1] - a.pos[1];
    let len = dx.hypot(dy);
    if len <= f32::EPSILON {
        return [0.0, 0.0];
    }
    [-dy / len * half_width, dx / len * half_width]
}

fn actor_multi_vertex_line_join_offset(
    prev: SongLuaActorMultiVertexPoint,
    current: SongLuaActorMultiVertexPoint,
    next: SongLuaActorMultiVertexPoint,
    half_width: f32,
) -> [f32; 2] {
    let prev_normal = actor_multi_vertex_line_normal(prev, current, 1.0);
    let next_normal = actor_multi_vertex_line_normal(current, next, 1.0);
    let miter = [
        prev_normal[0] + next_normal[0],
        prev_normal[1] + next_normal[1],
    ];
    let miter_len = miter[0].hypot(miter[1]);
    if miter_len <= f32::EPSILON {
        return [prev_normal[0] * half_width, prev_normal[1] * half_width];
    }
    let miter = [miter[0] / miter_len, miter[1] / miter_len];
    let denom = miter[0] * prev_normal[0] + miter[1] * prev_normal[1];
    if denom.abs() <= 0.1 {
        return [next_normal[0] * half_width, next_normal[1] * half_width];
    }
    let scale = (half_width / denom).clamp(-half_width * 4.0, half_width * 4.0);
    [miter[0] * scale, miter[1] * scale]
}

fn actor_multi_vertex_offset_point(
    vertex: SongLuaActorMultiVertexPoint,
    offset: [f32; 2],
    sign: f32,
) -> SongLuaActorMultiVertexPoint {
    SongLuaActorMultiVertexPoint {
        pos: [
            vertex.pos[0] + offset[0] * sign,
            vertex.pos[1] + offset[1] * sign,
        ],
        color: vertex.color,
        uv: vertex.uv,
    }
}

fn push_actor_multi_vertex_quad(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
    d: SongLuaActorMultiVertexPoint,
) {
    push_actor_multi_vertex_triangle(out, a, b, c);
    push_actor_multi_vertex_triangle(out, c, d, a);
}

fn push_actor_multi_vertex_triangle(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
) {
    out.push(actor_multi_vertex_mesh_vertex(a));
    out.push(actor_multi_vertex_mesh_vertex(b));
    out.push(actor_multi_vertex_mesh_vertex(c));
}

#[inline(always)]
fn actor_multi_vertex_mesh_vertex(
    vertex: SongLuaActorMultiVertexPoint,
) -> SongLuaOverlayMeshVertex {
    SongLuaOverlayMeshVertex {
        pos: vertex.pos,
        color: vertex.color,
        uv: vertex.uv,
    }
}

fn collect_aft_capture_names(actor: &Table, out: &mut HashSet<String>) -> Result<(), String> {
    if actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
        && let Some(capture_name) = actor_aft_capture_name(actor).map_err(|err| err.to_string())?
    {
        out.insert(capture_name);
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        collect_aft_capture_names(&child, out)?;
    }
    Ok(())
}

fn actor_aft_capture_name(actor: &Table) -> mlua::Result<Option<String>> {
    if let Some(capture_name) = actor
        .get::<Option<String>>("__songlua_aft_capture_name")?
        .filter(|name| !name.trim().is_empty())
    {
        return Ok(Some(capture_name));
    }
    Ok(actor
        .get::<Option<String>>("Name")?
        .filter(|name| !name.trim().is_empty()))
}

fn actor_overlay_initial_state(actor: &Table) -> Result<SongLuaOverlayState, String> {
    let mut state = SongLuaOverlayState::default();
    if let Some(visible) = actor
        .get::<Option<bool>>("__songlua_visible")
        .map_err(|err| err.to_string())?
    {
        state.visible = visible;
    }
    if let Some(diffuse) = actor
        .get::<Option<Table>>("__songlua_state_diffuse")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.diffuse = diffuse;
    }
    if let Some(colors) = actor
        .get::<Option<Table>>("__songlua_state_vertex_colors")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vertex_colors(&value))
    {
        state.vertex_colors = Some(colors);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_x")
        .map_err(|err| err.to_string())?
    {
        state.x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_y")
        .map_err(|err| err.to_string())?
    {
        state.y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_z")
        .map_err(|err| err.to_string())?
    {
        state.z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_z_bias")
        .map_err(|err| err.to_string())?
    {
        state.z_bias = value;
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_draw_order")
        .map_err(|err| err.to_string())?
    {
        state.draw_order = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_draw_by_z_position")
        .map_err(|err| err.to_string())?
    {
        state.draw_by_z_position = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_halign")
        .map_err(|err| err.to_string())?
    {
        state.halign = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_valign")
        .map_err(|err| err.to_string())?
    {
        state.valign = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_text_align")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_text_align)
    {
        state.text_align = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_uppercase")
        .map_err(|err| err.to_string())?
    {
        state.uppercase = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_shadow_len")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.shadow_len = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_shadow_color")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.shadow_color = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_glow")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.glow = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fov")
        .map_err(|err| err.to_string())?
    {
        state.fov = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_vanishpoint")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropleft")
        .map_err(|err| err.to_string())?
    {
        state.cropleft = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropright")
        .map_err(|err| err.to_string())?
    {
        state.cropright = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_croptop")
        .map_err(|err| err.to_string())?
    {
        state.croptop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropbottom")
        .map_err(|err| err.to_string())?
    {
        state.cropbottom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadeleft")
        .map_err(|err| err.to_string())?
    {
        state.fadeleft = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_faderight")
        .map_err(|err| err.to_string())?
    {
        state.faderight = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadetop")
        .map_err(|err| err.to_string())?
    {
        state.fadetop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadebottom")
        .map_err(|err| err.to_string())?
    {
        state.fadebottom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mask_source")
        .map_err(|err| err.to_string())?
    {
        state.mask_source = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mask_dest")
        .map_err(|err| err.to_string())?
    {
        state.mask_dest = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_depth_test")
        .map_err(|err| err.to_string())?
    {
        state.depth_test = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom")
        .map_err(|err| err.to_string())?
    {
        state.zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_x")
        .map_err(|err| err.to_string())?
    {
        state.zoom_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_y")
        .map_err(|err| err.to_string())?
    {
        state.zoom_y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_z")
        .map_err(|err| err.to_string())?
    {
        state.zoom_z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom")
        .map_err(|err| err.to_string())?
    {
        state.basezoom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_x")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_y")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_z")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_x_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_x_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_y_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_y_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_z_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_z_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_skew_x")
        .map_err(|err| err.to_string())?
    {
        state.skew_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_skew_y")
        .map_err(|err| err.to_string())?
    {
        state.skew_y = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_vibrate")
        .map_err(|err| err.to_string())?
    {
        state.vibrate = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_magnitude")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec3(&value))
    {
        state.effect_magnitude = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_effect_clock")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_effect_clock)
    {
        state.effect_clock = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_color1")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.effect_color1 = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_color2")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.effect_color2 = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_effect_period")
        .map_err(|err| err.to_string())?
    {
        state.effect_period = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_effect_offset")
        .map_err(|err| err.to_string())?
    {
        state.effect_offset = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_timing")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec5(&value))
    {
        state.effect_timing = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_rainbow")
        .map_err(|err| err.to_string())?
    {
        state.rainbow = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_rainbow_scroll")
        .map_err(|err| err.to_string())?
    {
        state.rainbow_scroll = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_text_jitter")
        .map_err(|err| err.to_string())?
    {
        state.text_jitter = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_text_distortion")
        .map_err(|err| err.to_string())?
    {
        state.text_distortion = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_text_glow_mode")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_text_glow_mode)
    {
        state.text_glow_mode = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mult_attrs_with_diffuse")
        .map_err(|err| err.to_string())?
    {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_sprite_animate")
        .map_err(|err| err.to_string())?
    {
        state.sprite_animate = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_sprite_loop")
        .map_err(|err| err.to_string())?
    {
        state.sprite_loop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_sprite_playback_rate")
        .map_err(|err| err.to_string())?
    {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_sprite_state_delay")
        .map_err(|err| err.to_string())?
    {
        state.sprite_state_delay = value;
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_vert_spacing")
        .map_err(|err| err.to_string())?
    {
        state.vert_spacing = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_wrap_width_pixels")
        .map_err(|err| err.to_string())?
    {
        state.wrap_width_pixels = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_max_width")
        .map_err(|err| err.to_string())?
    {
        state.max_width = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_max_height")
        .map_err(|err| err.to_string())?
    {
        state.max_height = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_w_pre_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_h_pre_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_dimension_uses_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<u32>>("__songlua_state_sprite_state_index")
        .map_err(|err| err.to_string())?
    {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_decode_movie")
        .map_err(|err| err.to_string())?
    {
        state.decode_movie = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_texture_filtering")
        .map_err(|err| err.to_string())?
    {
        state.texture_filtering = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_texture_wrapping")
        .map_err(|err| err.to_string())?
    {
        state.texture_wrapping = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_texcoord_offset")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_custom_texture_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_texcoord_velocity")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.size = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_stretch_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.stretch_rect = Some(value);
    }
    if let Some(raw) = actor
        .get::<Option<String>>("__songlua_state_blend")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_blend_mode)
    {
        state.blend = raw;
    }
    if let Some(raw) = actor
        .get::<Option<String>>("__songlua_state_effect_mode")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_effect_mode)
    {
        state.effect_mode = raw;
    }
    Ok(state)
}

fn read_proxy_target_kind(actor: &Table) -> Result<Option<SongLuaProxyTarget>, String> {
    let Some(raw_kind) = actor
        .get::<Option<String>>("__songlua_proxy_target_kind")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let player_index = actor
        .get::<Option<i64>>("__songlua_proxy_player_index")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok());
    Ok(match raw_kind.as_str() {
        "player" => player_index.map(|player_index| SongLuaProxyTarget::Player { player_index }),
        "notefield" => {
            player_index.map(|player_index| SongLuaProxyTarget::NoteField { player_index })
        }
        "judgment" => {
            player_index.map(|player_index| SongLuaProxyTarget::Judgment { player_index })
        }
        "combo" => player_index.map(|player_index| SongLuaProxyTarget::Combo { player_index }),
        "underlay" => Some(SongLuaProxyTarget::Underlay),
        "overlay" => Some(SongLuaProxyTarget::Overlay),
        _ => None,
    })
}

fn resolve_actor_asset_path(actor: &Table, raw: &str) -> Result<PathBuf, String> {
    let raw_path = Path::new(raw.trim());
    if raw_path.is_absolute() && raw_path.is_file() {
        return Ok(raw_path.to_path_buf());
    }
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")
        .map_err(|err| err.to_string())?
        .filter(|dir| !dir.trim().is_empty())
    {
        let candidate = Path::new(&script_dir).join(raw);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("actor asset '{}' could not be resolved", raw))
}

fn read_bitmap_font(actor: &Table) -> Result<Option<(&'static str, PathBuf)>, String> {
    let Some(font) = actor
        .get::<Option<String>>("Font")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    if let Ok(font_path) = resolve_actor_asset_path(actor, &font) {
        return Ok(Some((song_lua_font_name(font_path.as_path()), font_path)));
    }
    if song_lua_uses_builtin_theme_font(&font) {
        return Ok(Some((
            "miso",
            PathBuf::from("assets/fonts/miso/_miso light.ini"),
        )));
    }
    Ok(None)
}

fn read_bitmap_text_attributes(actor: &Table) -> Result<Arc<[TextAttribute]>, String> {
    let Some(attributes) = actor
        .get::<Option<Table>>("__songlua_text_attributes")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Arc::<[TextAttribute]>::from([]));
    };
    let mut out = Vec::with_capacity(attributes.raw_len());
    for entry in attributes.sequence_values::<Table>() {
        let entry = entry.map_err(|err| err.to_string())?;
        let start_value = entry
            .raw_get::<Value>("start")
            .map_err(|err| err.to_string())?;
        let start = read_i32_value(start_value).unwrap_or(0).max(0) as usize;
        let length_value = entry
            .raw_get::<Value>("length")
            .map_err(|err| err.to_string())?;
        let length_raw = read_i32_value(length_value).unwrap_or(1);
        if length_raw == 0 {
            continue;
        }
        let length = if length_raw < 0 {
            usize::MAX
        } else {
            length_raw as usize
        };
        let color = entry
            .raw_get::<Table>("color")
            .ok()
            .and_then(|color| table_vec4(&color))
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let vertex_colors = entry
            .raw_get::<Table>("vertex_colors")
            .ok()
            .and_then(|colors| table_vertex_colors(&colors));
        let glow = entry
            .raw_get::<Table>("glow")
            .ok()
            .and_then(|color| table_vec4(&color));
        out.push(TextAttribute {
            start,
            length,
            color,
            vertex_colors,
            glow,
        });
    }
    Ok(Arc::from(out.into_boxed_slice()))
}

fn song_lua_uses_builtin_theme_font(raw: &str) -> bool {
    let trimmed = raw.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    let normalized = trimmed.replace('\\', "/");
    normalized.starts_with(SONG_LUA_THEME_PATH_PREFIX)
        || (!normalized.contains('/') && Path::new(&normalized).extension().is_none())
}

fn read_actor_color_field(actor: &Table, key: &str) -> Result<Option<[f32; 4]>, String> {
    Ok(actor
        .get::<Option<Table>>(key)
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value)))
}

fn actor_shadow_len(lua: &Lua, actor: &Table) -> mlua::Result<[f32; 2]> {
    let block = actor_current_capture_block(lua, actor)?;
    if let Some(value) = block
        .get::<Option<Table>>("shadow_len")?
        .and_then(|value| table_vec2(&value))
    {
        return Ok(value);
    }
    Ok(actor
        .get::<Option<Table>>("__songlua_state_shadow_len")?
        .and_then(|value| table_vec2(&value))
        .unwrap_or([0.0, 0.0]))
}

fn song_lua_font_name(font_path: &Path) -> &'static str {
    static SONG_LUA_FONT_NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let canonical = font_path.to_string_lossy().replace('\\', "/");
    let cache = SONG_LUA_FONT_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&name) = guard.get(&canonical) {
        return name;
    }
    let leaked = Box::leak(format!("songlua_font:{canonical}").into_boxed_str());
    guard.insert(canonical, leaked);
    leaked
}

fn capture_actor_command(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(command) = actor
        .get::<Option<Function>>(command_name)
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    let params =
        default_message_command_params(lua, command_name).map_err(|err| err.to_string())?;
    call_actor_function(lua, actor, &command, params).map_err(|err| err.to_string())?;
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    read_actor_capture_blocks(actor)
}

fn capture_actor_command_preserving_state(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let snapshot = snapshot_actor_mutable_state(lua, actor).map_err(|err| err.to_string())?;
    let captured = capture_actor_command(lua, actor, command_name);
    restore_actor_mutable_state(actor, snapshot).map_err(|err| err.to_string())?;
    captured
}

fn is_actor_mutable_state_key(key: &str) -> bool {
    key.starts_with("__songlua_state_")
        || key.starts_with("__songlua_capture_")
        || key.starts_with("__songlua_graph_display_")
        || key.starts_with("__songlua_scroller_")
        || matches!(
            key,
            "__songlua_visible"
                | "__songlua_diffuse"
                | "__songlua_text_attributes"
                | "__songlua_stroke_color"
                | "__songlua_aux"
                | "__songlua_aft_capture_name"
                | "__songlua_stream_width"
                | "__songlua_sprite_animation_length_seconds"
                | "__songlua_sprite_effect_mode"
                | "Text"
                | "Texture"
                | "File"
                | "StreamWidth"
        )
}

fn is_actor_semantic_state_key(key: &str) -> bool {
    is_actor_mutable_state_key(key) && !key.starts_with("__songlua_capture_")
}

fn snapshot_actor_mutable_state(lua: &Lua, actor: &Table) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_mutable_state_key)
}

fn snapshot_actor_semantic_state(lua: &Lua, actor: &Table) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_semantic_state_key)
}

fn snapshot_actor_state(
    lua: &Lua,
    actor: &Table,
    keep_key: fn(&str) -> bool,
) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?.to_string();
        if keep_key(&key) {
            out.push((key, clone_lua_value(lua, value)?));
        }
    }
    Ok(out)
}

fn restore_actor_mutable_state(actor: &Table, snapshot: Vec<(String, Value)>) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_mutable_state_key)
}

fn restore_actor_semantic_state(actor: &Table, snapshot: Vec<(String, Value)>) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_semantic_state_key)
}

fn restore_actor_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
    clear_key: fn(&str) -> bool,
) -> mlua::Result<()> {
    let mut keys = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?.to_string();
        if clear_key(&key) {
            keys.push(key);
        }
    }
    for key in keys {
        actor.set(key, Value::Nil)?;
    }
    for (key, value) in snapshot {
        actor.set(key, value)?;
    }
    Ok(())
}

fn snapshot_actors_semantic_state(
    lua: &Lua,
    actors: &[Table],
) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    actors
        .iter()
        .map(|actor| Ok((actor.clone(), snapshot_actor_semantic_state(lua, actor)?)))
        .collect()
}

fn restore_actors_semantic_state(
    snapshots: Vec<(Table, Vec<(String, Value)>)>,
) -> mlua::Result<()> {
    for (actor, snapshot) in snapshots {
        restore_actor_semantic_state(&actor, snapshot)?;
    }
    Ok(())
}

fn snapshot_actor_semantic_state_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let snapshot = lua.create_table()?;
    for (index, (key, value)) in snapshot_actor_semantic_state(lua, actor)?
        .into_iter()
        .enumerate()
    {
        let entry = lua.create_table()?;
        entry.raw_set(1, key)?;
        entry.raw_set(2, value)?;
        snapshot.raw_set(index + 1, entry)?;
    }
    Ok(snapshot)
}

fn read_actor_semantic_state_table(snapshot: &Table) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::with_capacity(snapshot.raw_len());
    for entry in snapshot.sequence_values::<Table>() {
        let entry = entry?;
        out.push((entry.raw_get(1)?, entry.raw_get(2)?));
    }
    Ok(out)
}

fn reset_actor_capture(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set("__songlua_capture_cursor", 0.0_f32)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_tween_time_left", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    actor.set("__songlua_capture_blocks", lua.create_table()?)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

fn actor_current_capture_block(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    record_probe_actor_call(lua, actor)?;
    prepare_capture_scope_actor(lua, actor)?;
    if let Some(block) = actor.get::<Option<Table>>("__songlua_capture_block")? {
        return Ok(block);
    }
    let block = lua.create_table()?;
    let start = actor
        .get::<Option<f32>>("__songlua_capture_cursor")?
        .unwrap_or(0.0);
    let duration = actor
        .get::<Option<f32>>("__songlua_capture_duration")?
        .unwrap_or(0.0)
        .max(0.0);
    block.set("start", start)?;
    block.set("duration", duration)?;
    block.set("easing", actor.get::<Value>("__songlua_capture_easing")?)?;
    block.set("opt1", actor.get::<Value>("__songlua_capture_opt1")?)?;
    block.set("opt2", actor.get::<Value>("__songlua_capture_opt2")?)?;
    block.set("__songlua_has_changes", false)?;
    actor.set("__songlua_capture_block", block.clone())?;
    Ok(block)
}

fn flush_actor_capture(actor: &Table) -> mlua::Result<()> {
    let Some(block) = actor.get::<Option<Table>>("__songlua_capture_block")? else {
        return Ok(());
    };
    let duration = block
        .get::<Option<f32>>("duration")?
        .unwrap_or(0.0)
        .max(0.0);
    let cursor = block.get::<Option<f32>>("start")?.unwrap_or(0.0);
    if block
        .get::<Option<bool>>("__songlua_has_changes")?
        .unwrap_or(false)
    {
        let blocks: Table = actor.get("__songlua_capture_blocks")?;
        let index = blocks.raw_len() + 1;
        blocks.raw_set(index, block.clone())?;
    }
    actor.set("__songlua_capture_cursor", cursor + duration)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

fn actor_tween_time_left(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_capture_tween_time_left")?
        .unwrap_or(0.0)
        .max(0.0))
}

fn scale_actor_capture_f32(actor: &Table, key: &str, scale: f32) -> mlua::Result<()> {
    let value = actor.get::<Option<f32>>(key)?.unwrap_or(0.0);
    actor.set(key, value * scale)
}

fn hurry_actor_tweening(actor: &Table, factor: f32) -> mlua::Result<()> {
    if !factor.is_finite() || factor <= f32::EPSILON {
        return Ok(());
    }
    flush_actor_capture(actor)?;
    let scale = factor.recip();
    if let Some(blocks) = actor.get::<Option<Table>>("__songlua_capture_blocks")? {
        for block in blocks.sequence_values::<Table>() {
            let block = block?;
            let start = block.get::<Option<f32>>("start")?.unwrap_or(0.0);
            let duration = block.get::<Option<f32>>("duration")?.unwrap_or(0.0);
            block.set("start", start * scale)?;
            block.set("duration", duration * scale)?;
        }
    }
    scale_actor_capture_f32(actor, "__songlua_capture_cursor", scale)?;
    scale_actor_capture_f32(actor, "__songlua_capture_duration", scale)?;
    scale_actor_capture_f32(actor, "__songlua_capture_tween_time_left", scale)
}

fn is_capture_block_meta_key(key: &str) -> bool {
    matches!(
        key,
        "start" | "duration" | "easing" | "opt1" | "opt2" | "__songlua_has_changes"
    )
}

fn finish_actor_tweening(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    flush_actor_capture(actor)?;
    let final_block = lua.create_table()?;
    let mut has_changes = false;
    if let Some(blocks) = actor.get::<Option<Table>>("__songlua_capture_blocks")? {
        for block in blocks.sequence_values::<Table>() {
            for pair in block?.pairs::<Value, Value>() {
                let (key, value) = pair?;
                if matches!(
                    &key,
                    Value::String(text)
                        if text
                            .to_str()
                            .ok()
                            .is_some_and(|name| is_capture_block_meta_key(&name))
                ) {
                    continue;
                }
                final_block.set(clone_lua_value(lua, key)?, clone_lua_value(lua, value)?)?;
                has_changes = true;
            }
        }
    }

    let blocks = lua.create_table()?;
    if has_changes {
        final_block.set("start", 0.0_f32)?;
        final_block.set("duration", 0.0_f32)?;
        final_block.set("easing", Value::Nil)?;
        final_block.set("opt1", Value::Nil)?;
        final_block.set("opt2", Value::Nil)?;
        final_block.set("__songlua_has_changes", true)?;
        blocks.raw_set(1, final_block)?;
    }
    actor.set("__songlua_capture_blocks", blocks)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    actor.set("__songlua_capture_cursor", 0.0_f32)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_tween_time_left", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    Ok(())
}

fn capture_block_set_f32(lua: &Lua, actor: &Table, key: &str, value: f32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_bool(lua: &Lua, actor: &Table, key: &str, value: bool) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_color(lua: &Lua, actor: &Table, color: [f32; 4]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, color[0])?;
    value.raw_set(2, color[1])?;
    value.raw_set(3, color[2])?;
    value.raw_set(4, color[3])?;
    block.set("diffuse", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_diffuse", value.clone())?;
    actor.set("__songlua_state_diffuse", value)?;
    Ok(())
}

fn actor_diffuse(actor: &Table) -> mlua::Result<[f32; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_diffuse")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0, 1.0, 1.0, 1.0]))
}

fn actor_vertex_colors(actor: &Table) -> mlua::Result<[[f32; 4]; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_vertex_colors")?
        .and_then(|value| table_vertex_colors(&value))
        .unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]))
}

fn capture_actor_vertex_diffuse(
    lua: &Lua,
    actor: &Table,
    args: &MultiValue,
    corner_mask: u8,
) -> mlua::Result<()> {
    let Some(color) = read_color_args(args) else {
        return Ok(());
    };
    let mut colors = actor_vertex_colors(actor)?;
    for (index, vertex_color) in colors.iter_mut().enumerate() {
        if corner_mask & (1 << index) != 0 {
            *vertex_color = color;
        }
    }
    capture_block_set_vertex_colors(lua, actor, colors)
}

fn actor_text_attributes_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(attributes) = actor.get::<Option<Table>>("__songlua_text_attributes")? {
        return Ok(attributes);
    }
    let attributes = lua.create_table()?;
    actor.set("__songlua_text_attributes", attributes.clone())?;
    Ok(attributes)
}

fn capture_actor_text_attribute(lua: &Lua, actor: &Table, args: &MultiValue) -> mlua::Result<()> {
    let Some(start) = method_arg(args, 0).cloned().and_then(read_i32_value) else {
        return Ok(());
    };
    let Some(Value::Table(params)) = method_arg(args, 1).cloned() else {
        return Ok(());
    };
    let length = text_attribute_value(&params, &["Length", "length"])?
        .and_then(read_i32_value)
        .unwrap_or(1);
    if length == 0 {
        return Ok(());
    }
    let mut vertex_colors = text_attribute_value(&params, &["Diffuses", "diffuses"])?
        .and_then(read_vertex_colors_value)
        .unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
    if let Some(color) = text_attribute_value(
        &params,
        &[
            "Diffuse",
            "diffuse",
            "DiffuseColor",
            "diffusecolor",
            "Color",
            "color",
        ],
    )?
    .and_then(read_color_value)
    {
        vertex_colors = [color; 4];
    }
    let color = vertex_colors[0];
    let glow = text_attribute_value(&params, &["Glow", "glow"])?.and_then(read_color_value);

    let attr = lua.create_table()?;
    attr.raw_set("start", start.max(0))?;
    attr.raw_set("length", length)?;
    attr.raw_set("color", make_color_table(lua, color)?)?;
    if vertex_colors != [color; 4] {
        attr.raw_set(
            "vertex_colors",
            make_vertex_color_table(lua, vertex_colors)?,
        )?;
    }
    if let Some(glow) = glow {
        attr.raw_set("glow", make_color_table(lua, glow)?)?;
    }
    let attributes = actor_text_attributes_table(lua, actor)?;
    for existing in attributes.sequence_values::<Table>() {
        if text_attribute_matches(&existing?, start.max(0), length, color, vertex_colors, glow)? {
            return Ok(());
        }
    }
    attributes.raw_set(attributes.raw_len() + 1, attr)?;
    Ok(())
}

fn text_attribute_matches(
    attr: &Table,
    start: i32,
    length: i32,
    color: [f32; 4],
    vertex_colors: [[f32; 4]; 4],
    glow: Option<[f32; 4]>,
) -> mlua::Result<bool> {
    if attr.raw_get::<Option<i32>>("start")?.unwrap_or(-1) != start
        || attr.raw_get::<Option<i32>>("length")?.unwrap_or(0) != length
    {
        return Ok(false);
    }
    let existing_color = attr
        .raw_get::<Option<Table>>("color")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0; 4]);
    let existing_vertex_colors = attr
        .raw_get::<Option<Table>>("vertex_colors")?
        .and_then(|value| table_vertex_colors(&value))
        .unwrap_or([existing_color; 4]);
    let existing_glow = attr
        .raw_get::<Option<Table>>("glow")?
        .and_then(|value| table_vec4(&value));
    Ok(existing_color == color && existing_vertex_colors == vertex_colors && existing_glow == glow)
}

fn text_attribute_value(params: &Table, keys: &[&str]) -> mlua::Result<Option<Value>> {
    for key in keys {
        let value = params.get::<Value>(*key)?;
        if !matches!(value, Value::Nil) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn actor_glow(actor: &Table) -> mlua::Result<[f32; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_glow")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([0.0, 0.0, 0.0, 0.0]))
}

fn actor_effect_magnitude(actor: &Table) -> mlua::Result<[f32; 3]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_effect_magnitude")?
        .and_then(|value| table_vec3(&value))
        .unwrap_or([0.0, 0.0, 0.0]))
}

fn capture_block_set_vec4(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value4: [f32; 4],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value4[0])?;
    value.raw_set(2, value4[1])?;
    value.raw_set(3, value4[2])?;
    value.raw_set(4, value4[3])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_vertex_colors(
    lua: &Lua,
    actor: &Table,
    colors: [[f32; 4]; 4],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = make_vertex_color_table(lua, colors)?;
    block.set("vertex_colors", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_vertex_colors", value)?;
    Ok(())
}

fn capture_block_set_vec5(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value5: [f32; 5],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value5[0])?;
    value.raw_set(2, value5[1])?;
    value.raw_set(3, value5[2])?;
    value.raw_set(4, value5[3])?;
    value.raw_set(5, value5[4])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_u32(lua: &Lua, actor: &Table, key: &str, value: u32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_i32(lua: &Lua, actor: &Table, key: &str, value: i32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_vec2(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value2: [f32; 2],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value2[0])?;
    value.raw_set(2, value2[1])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_stretch(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, rect[0])?;
    value.raw_set(2, rect[1])?;
    value.raw_set(3, rect[2])?;
    value.raw_set(4, rect[3])?;
    block.set("stretch_rect", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_stretch_rect", value)?;
    Ok(())
}

fn capture_block_set_size(lua: &Lua, actor: &Table, size: [f32; 2]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, size[0])?;
    value.raw_set(2, size[1])?;
    block.set("size", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_size", value)?;
    Ok(())
}

fn capture_block_set_zoom_axes(
    lua: &Lua,
    actor: &Table,
    zoom: f32,
    zoom_x_key: &str,
    zoom_y_key: &str,
    zoom_z_key: &str,
) -> mlua::Result<()> {
    capture_block_set_f32(lua, actor, zoom_x_key, zoom)?;
    capture_block_set_f32(lua, actor, zoom_y_key, zoom)?;
    capture_block_set_f32(lua, actor, zoom_z_key, zoom)?;
    Ok(())
}

fn capture_block_set_vec3(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value3: [f32; 3],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value3[0])?;
    value.raw_set(2, value3[1])?;
    value.raw_set(3, value3[2])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_string(lua: &Lua, actor: &Table, key: &str, value: &str) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

#[inline(always)]
fn effect_clock_label(clock: EffectClock) -> &'static str {
    match clock {
        EffectClock::Time => "time",
        EffectClock::Beat => "beat",
    }
}

#[inline(always)]
fn text_glow_mode_label(mode: SongLuaTextGlowMode) -> &'static str {
    match mode {
        SongLuaTextGlowMode::Inner => "inner",
        SongLuaTextGlowMode::Stroke => "stroke",
        SongLuaTextGlowMode::Both => "both",
    }
}

#[inline(always)]
fn song_lua_valid_sprite_state_index(index: Option<u32>) -> Option<u32> {
    index.filter(|&value| value != SONG_LUA_SPRITE_STATE_CLEAR)
}

fn actor_sprite_sheet_dims(actor: &Table) -> mlua::Result<Option<(u32, u32)>> {
    if let Some(path) = actor_texture_path(actor)? {
        return Ok(Some(crate::assets::parse_sprite_sheet_dims(
            path.to_string_lossy().as_ref(),
        )));
    }
    let Some(texture) = actor.get::<Option<String>>("Texture")? else {
        return Ok(None);
    };
    let texture = texture.trim();
    if texture.is_empty() {
        return Ok(None);
    }
    Ok(Some(crate::assets::parse_sprite_sheet_dims(texture)))
}

#[inline(always)]
fn sprite_sheet_rect(index: u32, cols: u32, rows: u32) -> [f32; 4] {
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

fn actor_texture_rect(actor: &Table) -> mlua::Result<[f32; 4]> {
    if let Some(rect) = actor
        .get::<Option<Table>>("__songlua_state_custom_texture_rect")?
        .and_then(|value| table_vec4(&value))
    {
        return Ok(rect);
    }
    if let Some(state_index) = song_lua_valid_sprite_state_index(
        actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
    ) && let Some((cols, rows)) = actor_sprite_sheet_dims(actor)?
    {
        return Ok(sprite_sheet_rect(state_index, cols, rows));
    }
    Ok([0.0, 0.0, 1.0, 1.0])
}

fn capture_texture_rect(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    capture_block_set_u32(
        lua,
        actor,
        "sprite_state_index",
        SONG_LUA_SPRITE_STATE_CLEAR,
    )?;
    capture_block_set_vec4(lua, actor, "custom_texture_rect", rect)
}

fn offset_texture_rect(lua: &Lua, actor: &Table, dx: f32, dy: f32) -> mlua::Result<()> {
    let [u0, v0, u1, v1] = actor_texture_rect(actor)?;
    capture_texture_rect(lua, actor, [u0 + dx, v0 + dy, u1 + dx, v1 + dy])
}

fn actor_halign(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_state_halign")?
        .unwrap_or(0.5))
}

fn actor_valign(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_state_valign")?
        .unwrap_or(0.5))
}

fn scale_actor_to_rect(lua: &Lua, actor: &Table, rect: [f32; 4], cover: bool) -> mlua::Result<()> {
    let width = rect[2] - rect[0];
    let height = rect[3] - rect[1];
    let (base_width, base_height) = actor_base_size(actor)?;
    if base_width.abs() <= f32::EPSILON || base_height.abs() <= f32::EPSILON {
        return Ok(());
    }
    let zoom_x = (width / base_width).abs();
    let zoom_y = (height / base_height).abs();
    let zoom = if cover {
        zoom_x.max(zoom_y)
    } else {
        zoom_x.min(zoom_y)
    };
    if !zoom.is_finite() {
        return Ok(());
    }

    capture_block_set_f32(lua, actor, "x", rect[0] + width * actor_halign(actor)?)?;
    capture_block_set_f32(lua, actor, "y", rect[1] + height * actor_valign(actor)?)?;
    capture_block_set_f32(lua, actor, "zoom", zoom)?;
    capture_block_set_zoom_axes(lua, actor, zoom, "zoom_x", "zoom_y", "zoom_z")?;
    actor_update_text_pre_zoom_flags(lua, actor, true, true)?;
    if width < 0.0 {
        capture_block_set_f32(lua, actor, "rot_y_deg", 180.0)?;
    }
    if height < 0.0 {
        capture_block_set_f32(lua, actor, "rot_x_deg", 180.0)?;
    }
    Ok(())
}

fn set_actor_sprite_state(lua: &Lua, actor: &Table, state_index: u32) -> mlua::Result<()> {
    capture_block_set_u32(lua, actor, "sprite_state_index", state_index)?;
    let block = actor_current_capture_block(lua, actor)?;
    block.set("custom_texture_rect", Value::Nil)?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_custom_texture_rect", Value::Nil)?;
    Ok(())
}

fn set_actor_seconds_into_animation(lua: &Lua, actor: &Table, seconds: f32) -> mlua::Result<()> {
    let delay = actor
        .get::<Option<f32>>("__songlua_state_sprite_state_delay")?
        .unwrap_or(0.1)
        .max(0.0);
    let frame_count = actor_sprite_frame_count(actor)?.max(1);
    let state = if delay <= f32::EPSILON {
        0
    } else {
        ((seconds.max(0.0) / delay).floor() as u32) % frame_count
    };
    set_actor_sprite_state(lua, actor, state)
}

fn set_actor_effect_mode(lua: &Lua, actor: &Table, mode: &str) -> mlua::Result<()> {
    capture_block_set_string(lua, actor, "effect_mode", mode)
}

fn set_actor_effect_defaults(
    lua: &Lua,
    actor: &Table,
    mode: &str,
    period: Option<f32>,
    magnitude: Option<[f32; 3]>,
    color1: Option<[f32; 4]>,
    color2: Option<[f32; 4]>,
) -> mlua::Result<()> {
    set_actor_effect_mode(lua, actor, mode)?;
    if let Some(value) = period {
        capture_block_set_f32(lua, actor, "effect_period", value)?;
    }
    if let Some(value) = magnitude {
        capture_block_set_vec3(lua, actor, "effect_magnitude", value)?;
    }
    if let Some(value) = color1 {
        capture_block_set_vec4(lua, actor, "effect_color1", value)?;
    }
    if let Some(value) = color2 {
        capture_block_set_vec4(lua, actor, "effect_color2", value)?;
    }
    Ok(())
}

fn actor_sprite_frame_count(actor: &Table) -> mlua::Result<u32> {
    let Some((cols, rows)) = actor_sprite_sheet_dims(actor)? else {
        return Ok(1);
    };
    Ok(cols.max(1).saturating_mul(rows.max(1)).max(1))
}

fn read_actor_capture_blocks(actor: &Table) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(blocks) = actor
        .get::<Option<Table>>("__songlua_capture_blocks")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in blocks.sequence_values::<Value>() {
        let Value::Table(block) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let start = block
            .get::<Option<f32>>("start")
            .map_err(|err| err.to_string())?
            .unwrap_or(0.0);
        let duration = block
            .get::<Option<f32>>("duration")
            .map_err(|err| err.to_string())?
            .unwrap_or(0.0)
            .max(0.0);
        let easing = block
            .get::<Option<String>>("easing")
            .map_err(|err| err.to_string())?;
        out.push(SongLuaOverlayCommandBlock {
            start,
            duration,
            easing,
            opt1: block
                .get::<Option<f32>>("opt1")
                .map_err(|err| err.to_string())?,
            opt2: block
                .get::<Option<f32>>("opt2")
                .map_err(|err| err.to_string())?,
            delta: SongLuaOverlayStateDelta {
                x: block
                    .get::<Option<f32>>("x")
                    .map_err(|err| err.to_string())?,
                y: block
                    .get::<Option<f32>>("y")
                    .map_err(|err| err.to_string())?,
                z: block
                    .get::<Option<f32>>("z")
                    .map_err(|err| err.to_string())?,
                z_bias: block
                    .get::<Option<f32>>("z_bias")
                    .map_err(|err| err.to_string())?,
                draw_order: block
                    .get::<Option<i32>>("draw_order")
                    .map_err(|err| err.to_string())?,
                draw_by_z_position: block
                    .get::<Option<bool>>("draw_by_z_position")
                    .map_err(|err| err.to_string())?,
                halign: block
                    .get::<Option<f32>>("halign")
                    .map_err(|err| err.to_string())?,
                valign: block
                    .get::<Option<f32>>("valign")
                    .map_err(|err| err.to_string())?,
                text_align: block
                    .get::<Option<String>>("text_align")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_text_align),
                uppercase: block
                    .get::<Option<bool>>("uppercase")
                    .map_err(|err| err.to_string())?,
                shadow_len: block
                    .get::<Option<Table>>("shadow_len")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                shadow_color: block
                    .get::<Option<Table>>("shadow_color")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                glow: block
                    .get::<Option<Table>>("glow")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                fov: block
                    .get::<Option<f32>>("fov")
                    .map_err(|err| err.to_string())?,
                vanishpoint: block
                    .get::<Option<Table>>("vanishpoint")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                diffuse: block
                    .get::<Option<Table>>("diffuse")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                vertex_colors: block
                    .get::<Option<Table>>("vertex_colors")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vertex_colors(&value)),
                visible: block
                    .get::<Option<bool>>("visible")
                    .map_err(|err| err.to_string())?,
                cropleft: block
                    .get::<Option<f32>>("cropleft")
                    .map_err(|err| err.to_string())?,
                cropright: block
                    .get::<Option<f32>>("cropright")
                    .map_err(|err| err.to_string())?,
                croptop: block
                    .get::<Option<f32>>("croptop")
                    .map_err(|err| err.to_string())?,
                cropbottom: block
                    .get::<Option<f32>>("cropbottom")
                    .map_err(|err| err.to_string())?,
                fadeleft: block
                    .get::<Option<f32>>("fadeleft")
                    .map_err(|err| err.to_string())?,
                faderight: block
                    .get::<Option<f32>>("faderight")
                    .map_err(|err| err.to_string())?,
                fadetop: block
                    .get::<Option<f32>>("fadetop")
                    .map_err(|err| err.to_string())?,
                fadebottom: block
                    .get::<Option<f32>>("fadebottom")
                    .map_err(|err| err.to_string())?,
                mask_source: block
                    .get::<Option<bool>>("mask_source")
                    .map_err(|err| err.to_string())?,
                mask_dest: block
                    .get::<Option<bool>>("mask_dest")
                    .map_err(|err| err.to_string())?,
                depth_test: block
                    .get::<Option<bool>>("depth_test")
                    .map_err(|err| err.to_string())?,
                zoom: block
                    .get::<Option<f32>>("zoom")
                    .map_err(|err| err.to_string())?,
                zoom_x: block
                    .get::<Option<f32>>("zoom_x")
                    .map_err(|err| err.to_string())?,
                zoom_y: block
                    .get::<Option<f32>>("zoom_y")
                    .map_err(|err| err.to_string())?,
                zoom_z: block
                    .get::<Option<f32>>("zoom_z")
                    .map_err(|err| err.to_string())?,
                basezoom: block
                    .get::<Option<f32>>("basezoom")
                    .map_err(|err| err.to_string())?,
                basezoom_x: block
                    .get::<Option<f32>>("basezoom_x")
                    .map_err(|err| err.to_string())?,
                basezoom_y: block
                    .get::<Option<f32>>("basezoom_y")
                    .map_err(|err| err.to_string())?,
                basezoom_z: block
                    .get::<Option<f32>>("basezoom_z")
                    .map_err(|err| err.to_string())?,
                rot_x_deg: block
                    .get::<Option<f32>>("rot_x_deg")
                    .map_err(|err| err.to_string())?,
                rot_y_deg: block
                    .get::<Option<f32>>("rot_y_deg")
                    .map_err(|err| err.to_string())?,
                rot_z_deg: block
                    .get::<Option<f32>>("rot_z_deg")
                    .map_err(|err| err.to_string())?,
                skew_x: block
                    .get::<Option<f32>>("skew_x")
                    .map_err(|err| err.to_string())?,
                skew_y: block
                    .get::<Option<f32>>("skew_y")
                    .map_err(|err| err.to_string())?,
                blend: block
                    .get::<Option<String>>("blend")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_blend_mode),
                vibrate: block
                    .get::<Option<bool>>("vibrate")
                    .map_err(|err| err.to_string())?,
                effect_magnitude: block
                    .get::<Option<Table>>("effect_magnitude")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec3(&value)),
                effect_clock: block
                    .get::<Option<String>>("effect_clock")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock),
                effect_mode: block
                    .get::<Option<String>>("effect_mode")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_effect_mode),
                effect_color1: block
                    .get::<Option<Table>>("effect_color1")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                effect_color2: block
                    .get::<Option<Table>>("effect_color2")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                effect_period: block
                    .get::<Option<f32>>("effect_period")
                    .map_err(|err| err.to_string())?,
                effect_offset: block
                    .get::<Option<f32>>("effect_offset")
                    .map_err(|err| err.to_string())?,
                effect_timing: block
                    .get::<Option<Table>>("effect_timing")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec5(&value)),
                rainbow: block
                    .get::<Option<bool>>("rainbow")
                    .map_err(|err| err.to_string())?,
                rainbow_scroll: block
                    .get::<Option<bool>>("rainbow_scroll")
                    .map_err(|err| err.to_string())?,
                text_jitter: block
                    .get::<Option<bool>>("text_jitter")
                    .map_err(|err| err.to_string())?,
                text_distortion: block
                    .get::<Option<f32>>("text_distortion")
                    .map_err(|err| err.to_string())?,
                text_glow_mode: block
                    .get::<Option<String>>("text_glow_mode")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_text_glow_mode),
                mult_attrs_with_diffuse: block
                    .get::<Option<bool>>("mult_attrs_with_diffuse")
                    .map_err(|err| err.to_string())?,
                sprite_animate: block
                    .get::<Option<bool>>("sprite_animate")
                    .map_err(|err| err.to_string())?,
                sprite_loop: block
                    .get::<Option<bool>>("sprite_loop")
                    .map_err(|err| err.to_string())?,
                sprite_playback_rate: block
                    .get::<Option<f32>>("sprite_playback_rate")
                    .map_err(|err| err.to_string())?,
                sprite_state_delay: block
                    .get::<Option<f32>>("sprite_state_delay")
                    .map_err(|err| err.to_string())?,
                sprite_state_index: block
                    .get::<Option<u32>>("sprite_state_index")
                    .map_err(|err| err.to_string())?,
                vert_spacing: block
                    .get::<Option<i32>>("vert_spacing")
                    .map_err(|err| err.to_string())?,
                wrap_width_pixels: block
                    .get::<Option<i32>>("wrap_width_pixels")
                    .map_err(|err| err.to_string())?,
                max_width: block
                    .get::<Option<f32>>("max_width")
                    .map_err(|err| err.to_string())?,
                max_height: block
                    .get::<Option<f32>>("max_height")
                    .map_err(|err| err.to_string())?,
                max_w_pre_zoom: block
                    .get::<Option<bool>>("max_w_pre_zoom")
                    .map_err(|err| err.to_string())?,
                max_h_pre_zoom: block
                    .get::<Option<bool>>("max_h_pre_zoom")
                    .map_err(|err| err.to_string())?,
                max_dimension_uses_zoom: block
                    .get::<Option<bool>>("max_dimension_uses_zoom")
                    .map_err(|err| err.to_string())?,
                texture_filtering: block
                    .get::<Option<bool>>("texture_filtering")
                    .map_err(|err| err.to_string())?,
                texture_wrapping: block
                    .get::<Option<bool>>("texture_wrapping")
                    .map_err(|err| err.to_string())?,
                texcoord_offset: block
                    .get::<Option<Table>>("texcoord_offset")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                custom_texture_rect: block
                    .get::<Option<Table>>("custom_texture_rect")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                texcoord_velocity: block
                    .get::<Option<Table>>("texcoord_velocity")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                size: block
                    .get::<Option<Table>>("size")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                stretch_rect: block
                    .get::<Option<Table>>("stretch_rect")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                sound_play: block
                    .get::<Option<bool>>("sound_play")
                    .map_err(|err| err.to_string())?,
            },
        });
    }
    Ok(out)
}

fn table_vec4(table: &Table) -> Option<[f32; 4]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<f32>(4).ok()?,
    ])
}

fn table_vertex_colors(table: &Table) -> Option<[[f32; 4]; 4]> {
    Some([
        table_vec4(&table.raw_get::<Table>(1).ok()?)?,
        table_vec4(&table.raw_get::<Table>(2).ok()?)?,
        table_vec4(&table.raw_get::<Table>(3).ok()?)?,
        table_vec4(&table.raw_get::<Table>(4).ok()?)?,
    ])
}

fn make_vertex_color_table(lua: &Lua, colors: [[f32; 4]; 4]) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    for (index, color) in colors.into_iter().enumerate() {
        out.raw_set(index + 1, make_color_table(lua, color)?)?;
    }
    Ok(out)
}

fn table_vec5(table: &Table) -> Option<[f32; 5]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<f32>(4).ok()?,
        table.raw_get::<f32>(5).ok()?,
    ])
}

fn table_vec2(table: &Table) -> Option<[f32; 2]> {
    Some([table.raw_get::<f32>(1).ok()?, table.raw_get::<f32>(2).ok()?])
}

fn table_vec3(table: &Table) -> Option<[f32; 3]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
    ])
}

fn song_lua_halign_value(value: &Value) -> Option<f32> {
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

fn song_lua_valign_value(value: &Value) -> Option<f32> {
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

fn song_lua_align_token(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
        .trim_start_matches("horizalign_")
        .trim_start_matches("vertalign_")
        .to_string()
}

fn song_lua_text_align_value(value: &Value) -> Option<TextAlign> {
    read_string(value.clone()).and_then(|raw| parse_overlay_text_align(raw.as_str()))
}

fn overlay_text_align_label(value: TextAlign) -> &'static str {
    match value {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

fn actor_type_is(actor: &Table, expected: &str) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case(expected)))
}

fn actor_is_bitmap_text(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| {
            kind.eq_ignore_ascii_case("BitmapText") || kind.eq_ignore_ascii_case("RollingNumbers")
        }))
}

fn actor_update_text_pre_zoom_flags(
    lua: &Lua,
    actor: &Table,
    update_width: bool,
    update_height: bool,
) -> mlua::Result<()> {
    if !actor_is_bitmap_text(actor)? {
        return Ok(());
    }
    if update_width && actor.get::<Option<bool>>("__songlua_text_saw_max_width")? == Some(true) {
        capture_block_set_bool(lua, actor, "max_w_pre_zoom", true)?;
    }
    if update_height && actor.get::<Option<bool>>("__songlua_text_saw_max_height")? == Some(true) {
        capture_block_set_bool(lua, actor, "max_h_pre_zoom", true)?;
    }
    Ok(())
}

fn resolve_script_path(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<PathBuf> {
    let raw = Path::new(path);
    if raw.is_absolute() && raw.exists() {
        return Ok(raw.to_path_buf());
    }
    let globals = lua.globals();
    if let Some(current_dir) = globals
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        let candidate = Path::new(&current_dir).join(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    let candidate = song_dir.join(path);
    if candidate.exists() {
        return Ok(candidate);
    }
    Err(mlua::Error::external(format!(
        "script '{}' not found relative to '{}'",
        path,
        song_dir.display()
    )))
}

fn call_with_script_dir<T>(
    lua: &Lua,
    script_dir: &Path,
    f: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("__songlua_script_dir")?;
    globals.set(
        "__songlua_script_dir",
        script_dir.to_string_lossy().as_ref(),
    )?;
    let result = f();
    globals.set("__songlua_script_dir", previous)?;
    result
}

fn call_with_script_path<T>(
    lua: &Lua,
    script_path: &str,
    f: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("__songlua_current_script_path")?;
    globals.set("__songlua_current_script_path", script_path)?;
    let result = f();
    globals.set("__songlua_current_script_path", previous)?;
    result
}

fn call_with_chunk_env<T>(
    lua: &Lua,
    chunk_env: &Table,
    f: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("__songlua_current_chunk_env")?;
    globals.set("__songlua_current_chunk_env", chunk_env.clone())?;
    let result = f();
    globals.set("__songlua_current_chunk_env", previous)?;
    result
}

fn create_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
    let actor = lua.create_table()?;
    actor.set("__songlua_actor_type", actor_type)?;
    inherit_actor_dirs(lua, &actor)?;
    install_actor_methods(lua, &actor)?;
    install_actor_metatable(lua, &actor)?;
    reset_actor_capture(lua, &actor)?;
    register_song_lua_actor(lua, &actor)?;
    Ok(actor)
}

fn create_media_actor(
    lua: &Lua,
    actor_type: &'static str,
    path: &str,
    resolved_path: &Path,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, actor_type)?;
    if actor_type.eq_ignore_ascii_case("Sound") {
        actor.set("File", path)?;
    } else {
        actor.set("Texture", file_path_string(resolved_path))?;
        set_actor_decode_movie_for_texture(&actor)?;
    }
    Ok(actor)
}

fn song_lua_actor_registry(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    if let Some(registry) = globals.get::<Option<Table>>("__songlua_actor_registry")? {
        return Ok(registry);
    }
    let registry = lua.create_table()?;
    globals.set("__songlua_actor_registry", registry.clone())?;
    Ok(registry)
}

fn register_song_lua_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let registry = song_lua_actor_registry(lua)?;
    let actor_ptr = actor.to_pointer() as usize;
    for value in registry.sequence_values::<Value>() {
        let Value::Table(existing) = value? else {
            continue;
        };
        if existing.to_pointer() as usize == actor_ptr {
            return Ok(());
        }
    }
    registry.raw_set(registry.raw_len() + 1, actor.clone())?;
    Ok(())
}

fn normalize_broadcast_params(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<Option<Value>> {
    match params {
        Some(Value::Table(table)) => {
            if message.eq_ignore_ascii_case("Judgment") {
                normalize_judgment_params(lua, &table)?;
            }
            Ok(Some(Value::Table(table)))
        }
        params => Ok(params),
    }
}

fn normalize_judgment_params(lua: &Lua, params: &Table) -> mlua::Result<()> {
    if matches!(params.get::<Value>("Player")?, Value::Nil) {
        params.set("Player", player_number_name(0))?;
    }
    if matches!(params.get::<Value>("TapNoteScore")?, Value::Nil) {
        params.set("TapNoteScore", "TapNoteScore_W3")?;
    }
    if matches!(params.get::<Value>("HoldNoteScore")?, Value::Nil) {
        params.set("HoldNoteScore", "HoldNoteScore_None")?;
    }
    if matches!(params.get::<Value>("TapNoteOffset")?, Value::Nil) {
        params.set("TapNoteOffset", 0.0_f32)?;
    }

    let notes = match params.get::<Value>("Notes")? {
        Value::Table(notes) => notes,
        _ => {
            let notes = lua.create_table()?;
            let first_track = table_i32_field(params, &["FirstTrack", "Column"])?.unwrap_or(0);
            notes.raw_set(
                i64::from(first_track.max(0)) + 1,
                create_tap_note_table(lua, params, None)?,
            )?;
            params.set("Notes", notes.clone())?;
            notes
        }
    };
    if table_has_entries(&notes)? {
        let entries = notes
            .pairs::<Value, Value>()
            .collect::<mlua::Result<Vec<_>>>()?;
        for (key, value) in entries {
            let note = match value {
                Value::Table(note) => normalize_tap_note_table(lua, params, note)?,
                value => create_tap_note_table(lua, params, Some(value))?,
            };
            notes.set(key, note)?;
        }
    } else {
        notes.raw_set(1, create_tap_note_table(lua, params, None)?)?;
    }
    Ok(())
}

fn default_message_command_params(lua: &Lua, command_name: &str) -> mlua::Result<Option<Value>> {
    let Some(message) = command_name.strip_suffix("MessageCommand") else {
        return Ok(None);
    };
    let params = lua.create_table()?;
    match message {
        "Judgment" => {
            params.set("Player", player_number_name(0))?;
            params.set("TapNoteScore", "TapNoteScore_W3")?;
            params.set("TapNoteOffset", 0.0_f32)?;
            params.set("FirstTrack", 0_i64)?;
            normalize_judgment_params(lua, &params)?;
        }
        "EarlyHit" => {
            params.set("Player", player_number_name(0))?;
            params.set("TapNoteScore", "TapNoteScore_W3")?;
            params.set("TapNoteOffset", -0.02_f32)?;
            params.set("Early", true)?;
        }
        "LifeChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("LifeMeter", create_life_meter_param_table(lua)?)?;
        }
        "HealthStateChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("PlayerNumber", player_number_name(0))?;
            params.set("HealthState", "HealthState_Alive")?;
        }
        "ExCountsChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("ExCounts", create_ex_counts_param_table(lua)?)?;
            params.set("ExScore", 0.0_f32)?;
            params.set("ActualPoints", 0.0_f32)?;
            params.set("ActualPossible", 1.0_f32)?;
            params.set("CurrentPossible", 1.0_f32)?;
        }
        "PlayerOptionsChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("PlayerNumber", player_number_name(0))?;
        }
        _ => return Ok(None),
    }
    Ok(Some(Value::Table(params)))
}

fn create_life_meter_param_table(lua: &Lua) -> mlua::Result<Table> {
    let meter = lua.create_table()?;
    meter.set(
        "GetLife",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_INITIAL_LIFE))?,
    )?;
    meter.set(
        "IsFailing",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    meter.set(
        "IsHot",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    meter.set(
        "IsInDanger",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(meter)
}

fn create_ex_counts_param_table(lua: &Lua) -> mlua::Result<Table> {
    let counts = lua.create_table()?;
    for key in [
        "W0", "W1", "W2", "W3", "W4", "W5", "Miss", "Held", "LetGo", "HitMine",
    ] {
        counts.set(key, 0_i64)?;
    }
    Ok(counts)
}

fn table_has_entries(table: &Table) -> mlua::Result<bool> {
    Ok(table.pairs::<Value, Value>().next().transpose()?.is_some())
}

fn normalize_tap_note_table(lua: &Lua, params: &Table, note: Table) -> mlua::Result<Table> {
    if !matches!(note.get::<Value>("GetTapNoteType")?, Value::Nil) {
        return Ok(note);
    }
    let result = match note.get::<Value>("TapNoteResult")? {
        Value::Table(result) => normalize_tap_note_result_table(lua, params, result, Some(&note))?,
        _ => create_tap_note_result_table(lua, params, Some(&note))?,
    };
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

fn create_tap_note_table(
    lua: &Lua,
    params: &Table,
    note_value: Option<Value>,
) -> mlua::Result<Table> {
    let note = lua.create_table()?;
    if let Some(value) = note_value {
        note.set("TapNoteType", value)?;
    }
    let result = create_tap_note_result_table(lua, params, Some(&note))?;
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

fn install_tap_note_methods(
    lua: &Lua,
    params: &Table,
    note: &Table,
    result: Table,
) -> mlua::Result<()> {
    let note_type = table_string_field(note, &["TapNoteType", "Type", "NoteType"])?
        .or(table_string_field(
            params,
            &["TapNoteType", "Type", "NoteType"],
        )?)
        .unwrap_or_else(|| "TapNoteType_Tap".to_string());
    let source = table_string_field(note, &["TapNoteSource", "Source"])?
        .unwrap_or_else(|| "TapNoteSource_Original".to_string());
    let subtype = table_string_field(note, &["TapNoteSubType", "SubType"])?
        .unwrap_or_else(|| "TapNoteSubType_Hold".to_string());
    let player = table_string_field(params, &["Player"])?
        .unwrap_or_else(|| player_number_name(0).to_string());
    let hold_duration = table_f32_field(note, &["HoldDuration"])?.unwrap_or(0.0);
    let attack_duration = table_f32_field(note, &["AttackDuration"])?.unwrap_or(0.0);
    let attack_mods = table_string_field(note, &["AttackModifiers"])?.unwrap_or_default();
    let keysound = table_i32_field(note, &["KeysoundIndex"])?.unwrap_or(0);

    set_string_method(lua, note, "GetTapNoteType", &note_type)?;
    set_string_method(lua, note, "GetTapNoteSource", &source)?;
    set_string_method(lua, note, "GetTapNoteSubType", &subtype)?;
    set_string_method(lua, note, "GetPlayerNumber", &player)?;
    set_string_method(lua, note, "GetAttackModifiers", &attack_mods)?;
    note.set(
        "GetTapNoteResult",
        lua.create_function({
            let result = result.clone();
            move |_, _args: MultiValue| Ok(result.clone())
        })?,
    )?;
    note.set(
        "GetHoldNoteResult",
        lua.create_function({
            let result = result.clone();
            move |_, _args: MultiValue| Ok(result.clone())
        })?,
    )?;
    note.set(
        "GetHoldDuration",
        lua.create_function(move |_, _args: MultiValue| Ok(hold_duration))?,
    )?;
    note.set(
        "GetAttackDuration",
        lua.create_function(move |_, _args: MultiValue| Ok(attack_duration))?,
    )?;
    note.set(
        "GetKeysoundIndex",
        lua.create_function(move |_, _args: MultiValue| Ok(keysound))?,
    )?;
    Ok(())
}

fn create_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    install_tap_note_result_methods(lua, params, note, &result)?;
    Ok(result)
}

fn normalize_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    result: Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    if matches!(result.get::<Value>("GetHeld")?, Value::Nil) {
        install_tap_note_result_methods(lua, params, note, &result)?;
    }
    Ok(result)
}

fn install_tap_note_result_methods(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
    result: &Table,
) -> mlua::Result<()> {
    let held = match note {
        Some(note) => table_bool_field(note, &["Held", "held"])?,
        None => None,
    }
    .unwrap_or(false);
    let hidden = match note {
        Some(note) => table_bool_field(note, &["Hidden", "hidden"])?,
        None => None,
    }
    .unwrap_or(false);
    let offset = match note {
        Some(note) => table_f32_field(note, &["TapNoteOffset", "Offset"])?,
        None => None,
    }
    .or(table_f32_field(params, &["TapNoteOffset", "Offset"])?)
    .unwrap_or(0.0);
    let score = match note {
        Some(note) => table_string_field(note, &["TapNoteScore", "Score"])?,
        None => None,
    }
    .or(table_string_field(params, &["TapNoteScore", "Score"])?)
    .unwrap_or_else(|| "TapNoteScore_None".to_string());

    result.set(
        "GetHeld",
        lua.create_function(move |_, _args: MultiValue| Ok(held))?,
    )?;
    result.set(
        "GetHidden",
        lua.create_function(move |_, _args: MultiValue| Ok(hidden))?,
    )?;
    result.set(
        "GetTapNoteOffset",
        lua.create_function(move |_, _args: MultiValue| Ok(offset))?,
    )?;
    set_string_method(lua, result, "GetTapNoteScore", &score)?;
    Ok(())
}

fn table_string_field(table: &Table, names: &[&str]) -> mlua::Result<Option<String>> {
    for name in names {
        if let Some(value) = read_string(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn table_f32_field(table: &Table, names: &[&str]) -> mlua::Result<Option<f32>> {
    for name in names {
        if let Some(value) = read_f32(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn table_i32_field(table: &Table, names: &[&str]) -> mlua::Result<Option<i32>> {
    for name in names {
        if let Some(value) = read_i32_value(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn table_bool_field(table: &Table, names: &[&str]) -> mlua::Result<Option<bool>> {
    for name in names {
        if let Some(value) = read_boolish(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn broadcast_song_lua_message(lua: &Lua, message: &str, params: Option<Value>) -> mlua::Result<()> {
    if message.trim().is_empty() {
        return Ok(());
    }
    let command = format!("{message}MessageCommand");
    let registry = song_lua_actor_registry(lua)?;
    let mut actors = Vec::with_capacity(registry.raw_len());
    for value in registry.sequence_values::<Value>() {
        let Value::Table(actor) = value? else {
            continue;
        };
        actors.push(actor);
    }
    let params = normalize_broadcast_params(lua, message, params)?;
    for actor in actors {
        run_actor_named_command_with_drain_and_params(lua, &actor, &command, true, params.clone())?;
    }
    Ok(())
}

fn inherit_actor_dirs(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(script_dir) = lua
        .globals()
        .get::<Option<String>>("__songlua_script_dir")?
    {
        actor.set("__songlua_script_dir", script_dir)?;
    }
    if let Some(song_dir) = lua.globals().get::<Option<String>>("__songlua_song_dir")? {
        actor.set("__songlua_song_dir", song_dir)?;
    }
    Ok(())
}

fn set_proxy_target_fields(actor: &Table, target: &Table) -> mlua::Result<()> {
    actor.set("__songlua_proxy_target_kind", Value::Nil)?;
    actor.set("__songlua_proxy_player_index", Value::Nil)?;
    if let Some(child_name) = target.get::<Option<String>>("__songlua_top_screen_child_name")? {
        let kind = if child_name.eq_ignore_ascii_case("Underlay") {
            Some("underlay")
        } else if child_name.eq_ignore_ascii_case("Overlay") {
            Some("overlay")
        } else {
            None
        };
        if let Some(kind) = kind {
            actor.set("__songlua_proxy_target_kind", kind)?;
        }
        return Ok(());
    }
    let Some(player_index) = target.get::<Option<i64>>("__songlua_player_index")? else {
        return Ok(());
    };
    actor.set("__songlua_proxy_player_index", player_index)?;
    let kind = match target
        .get::<Option<String>>("__songlua_player_child_name")?
        .as_deref()
    {
        Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
        Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
        Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
        _ => "player",
    };
    actor.set("__songlua_proxy_target_kind", kind)?;
    Ok(())
}

fn install_actor_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "SetTarget",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Table(target)) = args.get(1) {
                    set_proxy_target_fields(&actor, target)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "visible",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).map(truthy) {
                    prepare_capture_scope_actor(lua, &actor)?;
                    actor.set("__songlua_visible", value)?;
                    capture_block_set_bool(lua, &actor, "visible", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "addcommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, (_self, name, function): (Table, String, Function)| {
                actor.set(format!("{name}Command"), function)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "removecommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, (_self, name): (Table, String)| {
                actor.set(format!("{name}Command"), Value::Nil)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetCommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                Ok(actor
                    .get::<Option<Function>>(format!("{name}Command"))?
                    .map_or(Value::Nil, Value::Function))
            }
        })?,
    )?;
    actor.set(
        "GetTweenTimeLeft",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_tween_time_left(&actor)
        })?,
    )?;
    actor.set(
        "sleep",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                prepare_capture_scope_actor(lua, &actor)?;
                flush_actor_capture(&actor)?;
                let duration = args
                    .get(1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                let cursor = actor
                    .get::<Option<f32>>("__songlua_capture_cursor")?
                    .unwrap_or(0.0);
                actor.set("__songlua_capture_cursor", cursor + duration)?;
                actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "hibernate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                prepare_capture_scope_actor(lua, &actor)?;
                flush_actor_capture(&actor)?;
                let duration = args
                    .get(1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                if duration <= f32::EPSILON {
                    return Ok(actor.clone());
                }
                let restore_visible = actor
                    .get::<Option<bool>>("__songlua_visible")?
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "visible", false)?;
                flush_actor_capture(&actor)?;
                let cursor = actor
                    .get::<Option<f32>>("__songlua_capture_cursor")?
                    .unwrap_or(0.0);
                actor.set("__songlua_capture_cursor", cursor + duration)?;
                actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
                capture_block_set_bool(lua, &actor, "visible", restore_visible)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "linear",
        make_actor_tween_method(lua, actor, Some("linear"))?,
    )?;
    actor.set(
        "accelerate",
        make_actor_tween_method(lua, actor, Some("inQuad"))?,
    )?;
    actor.set(
        "decelerate",
        make_actor_tween_method(lua, actor, Some("outQuad"))?,
    )?;
    actor.set(
        "smooth",
        make_actor_tween_method(lua, actor, Some("inOutQuad"))?,
    )?;
    actor.set(
        "spring",
        make_actor_tween_method(lua, actor, Some("outElastic"))?,
    )?;
    actor.set(
        "bouncebegin",
        make_actor_tween_method(lua, actor, Some("inBounce"))?,
    )?;
    actor.set(
        "bounceend",
        make_actor_tween_method(lua, actor, Some("outBounce"))?,
    )?;
    actor.set(
        "queuecommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = args.get(1).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let active = actor_active_commands(lua, &actor)?;
                if active
                    .get::<Option<bool>>(format!("{name}Command"))?
                    .unwrap_or(false)
                {
                    return Ok(actor.clone());
                }
                let queue = actor_command_queue(lua, &actor)?;
                queue.raw_set(queue.raw_len() + 1, name)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                let command_name = format!("{name}Command");
                if actor
                    .get::<Option<bool>>("__songlua_propagate_commands")?
                    .unwrap_or(false)
                {
                    run_named_command_on_children_recursively(lua, &actor, &command_name, params)?;
                } else {
                    run_actor_named_command_with_drain_and_params(
                        lua,
                        &actor,
                        &command_name,
                        true,
                        params,
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "propagate",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                actor.set(
                    "__songlua_propagate_commands",
                    method_arg(&args, 0).is_some_and(truthy),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "propagatecommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let command = format!("{name}Command");
                run_named_command_on_children_recursively(
                    lua,
                    &actor,
                    &command,
                    method_arg(&args, 1).cloned(),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommandonchildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                let command = format!("{name}Command");
                for child in actor_direct_children(lua, &actor)? {
                    run_actor_named_command_with_drain_and_params(
                        lua,
                        &child,
                        &command,
                        true,
                        params.clone(),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommandonleaves",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                run_named_command_on_leaves(lua, &actor, &format!("{name}Command"), params)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "RemoveChild",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                remove_actor_child(lua, &actor, &name)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "RemoveAllChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                remove_all_actor_children(lua, &actor)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTexture",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                set_actor_texture_from_value(&actor, method_arg(&args, 0), false)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["Load", "LoadBanner", "LoadBackground"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                let method_name = name;
                move |_, args: MultiValue| {
                    if method_name == "Load" && actor_type_is(&actor, "RollingNumbers")? {
                        if let Some(metric) = method_arg(&args, 0).cloned().and_then(read_string) {
                            set_rolling_numbers_metric(&actor, &metric)?;
                        }
                        return Ok(actor.clone());
                    }
                    if method_name == "Load" && actor_type_is(&actor, "GraphDisplay")? {
                        if let Some(metric) = method_arg(&args, 0).cloned().and_then(read_string) {
                            actor.set("__songlua_graph_display_metric", metric)?;
                        }
                        return Ok(actor.clone());
                    }
                    if method_name == "Load" && actor_type_is(&actor, "Sound")? {
                        set_actor_sound_file_from_value(&actor, method_arg(&args, 0), true)?;
                        return Ok(actor.clone());
                    }
                    set_actor_texture_from_value(&actor, method_arg(&args, 0), true)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "LoadFromCached",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let path = method_arg(&args, 1)
                    .or_else(|| method_arg(&args, 0))
                    .filter(|value| !matches!(value, Value::Nil));
                set_actor_texture_from_value(&actor, path, true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in [
        "LoadFromCachedBanner",
        "LoadFromCachedBackground",
        "LoadFromCachedJacket",
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_value(&actor, method_arg(&args, 0), true)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    for (name, path_methods) in [
        ("LoadFromSong", &["GetBannerPath"][..]),
        (
            "LoadFromCourse",
            &["GetBannerPath", "GetBackgroundPath"][..],
        ),
        ("LoadFromSongBackground", &["GetBackgroundPath"][..]),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_path_methods(
                        &actor,
                        method_arg(&args, 0),
                        path_methods,
                    )?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "LoadFromSongGroup",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let group = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                let Some(songman) = lua.globals().get::<Option<Table>>("SONGMAN")? else {
                    return Ok(actor.clone());
                };
                let Some(method) = songman.get::<Option<Function>>("GetSongGroupBannerPath")?
                else {
                    return Ok(actor.clone());
                };
                if let Value::String(path) = method.call::<Value>((songman, group))? {
                    if !set_actor_texture_from_path(&actor, &path.to_str()?)? {
                        set_actor_texture_from_path(
                            &actor,
                            &theme_path("G", "Banner", "group fallback"),
                        )?;
                    }
                } else {
                    set_actor_texture_from_path(
                        &actor,
                        &theme_path("G", "Banner", "group fallback"),
                    )?;
                }
                actor.set("__songlua_state_banner_scrolling", false)?;
                actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "LoadFromSortOrder",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let sort_order = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_else(|| "SortOrder_Invalid".to_string());
                if let Some(path) = banner_sort_order_path(&sort_order) {
                    set_actor_texture_from_path(&actor, &path)?;
                }
                actor.set("__songlua_state_banner_scrolling", false)?;
                actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    const CHARACTER_ICON_PATH_METHODS: &[&str] = &["GetIconPath"];
    const CHARACTER_CARD_PATH_METHODS: &[&str] = &["GetCardPath"];
    const UNLOCK_BANNER_PATH_METHODS: &[&str] = &["GetBannerFile"];
    const UNLOCK_BACKGROUND_PATH_METHODS: &[&str] = &["GetBackgroundFile"];
    for (name, path_methods, fallback_path) in [
        (
            "LoadIconFromCharacter",
            CHARACTER_ICON_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadCardFromCharacter",
            CHARACTER_CARD_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadBannerFromUnlockEntry",
            UNLOCK_BANNER_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadBackgroundFromUnlockEntry",
            UNLOCK_BACKGROUND_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_path_methods_or_fallback(
                        &actor,
                        method_arg(&args, 0),
                        path_methods,
                        &fallback_path,
                    )?;
                    actor.set("__songlua_state_banner_scrolling", false)?;
                    actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let scrolling = method_arg(&args, 0).is_some_and(truthy);
                let percent = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                actor.set("__songlua_state_banner_scrolling", scrolling)?;
                actor.set("__songlua_state_banner_scroll_percent", percent)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_state_banner_scrolling")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "GetPercentScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_banner_scroll_percent")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetTextureName",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(name) = args.get(1).cloned().and_then(read_string) {
                    actor.set("__songlua_aft_capture_name", name)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetTexture",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Table| match actor.get::<Value>("Texture")? {
                Value::Nil
                    if !actor
                        .get::<Option<String>>("__songlua_actor_type")?
                        .as_deref()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture")) =>
                {
                    Ok(Value::Nil)
                }
                _ => Ok(Value::Table(create_texture_proxy(lua, &actor)?)),
            }
        })?,
    )?;
    actor.set(
        "x",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "x")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "x", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "y",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "y")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "y", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "fov",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "fov", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetFOV",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "fov", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetUpdateRate",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    if value.is_finite() && value > 0.0 {
                        actor.set("__songlua_update_rate", value)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetUpdateRate",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_update_rate")?
                    .unwrap_or(1.0))
            }
        })?,
    )?;
    actor.set(
        "vanishpoint",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(x) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(y) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec2(lua, &actor, "vanishpoint", [x, y])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "xy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(x) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "x", x)?;
                }
                if let Some(y) = args.get(2).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "y", y)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "halign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_halign_value) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "valign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "align",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_halign_value) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                if let Some(value) = method_arg(&args, 1).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "vertalign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "horizalign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(raw) = method_arg(&args, 0) else {
                    return Ok(actor.clone());
                };
                if let Some(value) = song_lua_halign_value(raw) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                if let Some(value) = song_lua_text_align_value(raw) {
                    capture_block_set_string(
                        lua,
                        &actor,
                        "text_align",
                        overlay_text_align_label(value),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "Center",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (center_x, center_y) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "x", center_x)?;
                capture_block_set_f32(lua, &actor, "y", center_y)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "CenterX",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (center_x, _) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "x", center_x)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "CenterY",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (_, center_y) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "y", center_y)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stretchto",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(3).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(4).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_stretch(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "FullScreen",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (width, height) = song_lua_screen_size(lua)?;
                capture_block_set_stretch(lua, &actor, [0.0, 0.0, width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "cropright",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "cropright", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "cropleft",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "cropleft", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffusealpha",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(alpha) = args.get(1).cloned().and_then(read_f32) {
                    let block = actor_current_capture_block(lua, &actor)?;
                    let mut diffuse = block
                        .get::<Option<Table>>("diffuse")?
                        .and_then(|value| table_vec4(&value))
                        .unwrap_or(actor_diffuse(&actor)?);
                    diffuse[3] = alpha;
                    capture_block_set_color(lua, &actor, diffuse)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom", value)?;
                    capture_block_set_zoom_axes(
                        lua,
                        &actor,
                        value,
                        "basezoom_x",
                        "basezoom_y",
                        "basezoom_z",
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_x", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_y", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomz",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_z", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "zoom")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "zoom", value)?;
                    capture_block_set_zoom_axes(lua, &actor, value, "zoom_x", "zoom_y", "zoom_z")?;
                    actor_update_text_pre_zoom_flags(lua, &actor, true, true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetUpdateFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                match args.get(1).cloned() {
                    Some(Value::Function(function)) => {
                        actor.set("__songlua_update_function", function)?;
                    }
                    _ => {
                        actor.set("__songlua_update_function", Value::Nil)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "aux",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_aux", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "getaux",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor.get::<Option<f32>>("__songlua_aux")?.unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "draworder",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    capture_block_set_i32(lua, &actor, "draw_order", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDrawFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Function(function)) = args.get(1).cloned() {
                    actor.set("__songlua_draw_function", function)?;
                } else {
                    actor.set("__songlua_draw_function", Value::Nil)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDrawFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<Function>>("__songlua_draw_function")?
                    .map_or(Value::Nil, Value::Function))
            }
        })?,
    )?;
    actor.set(
        "Set",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if actor_type_is(&actor, "GraphDisplay")? {
                    actor.set("__songlua_graph_display_set", true)?;
                    let stage_stats = method_arg(&args, 0).cloned();
                    let player_stats = method_arg(&args, 1).cloned();
                    if let Some(stage_stats) = stage_stats.clone() {
                        actor.set("__songlua_graph_display_stage_stats", stage_stats)?;
                    }
                    if let Some(player_stats) = player_stats.clone() {
                        actor.set("__songlua_graph_display_player_stats", player_stats)?;
                    }
                    capture_graph_display_values(lua, &actor, stage_stats, player_stats)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetStreamWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if actor_type_is(&actor, "SongMeterDisplay")?
                    && let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32)
                {
                    actor.set("StreamWidth", width)?;
                    actor.set("__songlua_stream_width", width)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetStreamWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_stream_width")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetFromGameState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if actor_type_is(&actor, "CourseContentsList")? {
                    actor.set("__songlua_course_contents_from_gamestate", true)?;
                    actor.set("__songlua_scroller_num_items", 1_i64)?;
                    populate_course_contents_display(lua, &actor)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCurrentAndDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_current_item", item)?;
                    actor.set("__songlua_scroller_destination_item", item)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCurrentItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_current_item", item)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetCurrentItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_current_item")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_destination_item", item.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_destination_item")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "GetNumItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .max(0))
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Function(function)) = method_arg(&args, 0).cloned() {
                    actor.set("__songlua_scroller_transform_function", function)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(height) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_item_height", height)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_item_width", width)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "PositionItems",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                position_scroller_items(lua, &actor)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetSecondsToDestination",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let current = actor
                    .get::<Option<f32>>("__songlua_scroller_current_item")?
                    .unwrap_or(0.0);
                let destination = actor
                    .get::<Option<f32>>("__songlua_scroller_destination_item")?
                    .unwrap_or(current);
                let seconds_per_item = actor
                    .get::<Option<f32>>("__songlua_scroller_seconds_per_item")?
                    .unwrap_or(0.0);
                Ok((destination - current).abs() * seconds_per_item)
            }
        })?,
    )?;
    actor.set(
        "SetSecondsPerItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_seconds_per_item", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetLoop",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).map(truthy) {
                    actor.set("__songlua_scroller_loop", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetLoop",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_scroller_loop")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "SetPauseCountdownSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_pause_countdown", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetSecondsPauseBetweenItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_pause_between", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetSecondsPauseBetweenItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_pause_between")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetNumSubdivisions",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(count) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    actor.set("__songlua_scroller_num_subdivisions", count.max(0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ScrollThroughAllItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let last_item = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .max(0) as f32;
                actor.set("__songlua_scroller_destination_item", last_item)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ScrollWithPadding",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let before = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                let after = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                let last_item = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .max(0) as f32;
                actor.set("__songlua_scroller_current_item", -before)?;
                actor.set("__songlua_scroller_destination_item", last_item + after)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for (name, key) in [
        ("SetFastCatchup", "__songlua_scroller_fast_catchup"),
        ("SetWrap", "__songlua_scroller_wrap"),
        ("SetMask", "__songlua_scroller_mask"),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    actor.set(key, method_arg(&args, 0).is_some_and(truthy))?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetNumItemsToDraw",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(count) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    actor.set("__songlua_scroller_num_items_to_draw", count.max(0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetFullScrollLengthSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let num_items = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .max(0) as f32;
                let seconds_per_item = actor
                    .get::<Option<f32>>("__songlua_scroller_seconds_per_item")?
                    .unwrap_or(0.0);
                let pause = actor
                    .get::<Option<f32>>("__songlua_scroller_pause_between")?
                    .unwrap_or(0.0);
                Ok(num_items * seconds_per_item + (num_items - 1.0).max(0.0) * pause)
            }
        })?,
    )?;
    actor.set(
        "SetDrawState",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Table(state)) = method_arg(&args, 0).cloned() {
                    actor.set("__songlua_draw_state", state.clone())?;
                    if let Some(mode) = state.get::<Option<String>>("Mode")? {
                        actor.set("__songlua_draw_state_mode", mode)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDrawState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if let Some(state) = actor.get::<Option<Table>>("__songlua_draw_state")? {
                    return Ok(state);
                }
                let state = lua.create_table()?;
                if let Some(mode) = actor.get::<Option<String>>("__songlua_draw_state_mode")? {
                    state.set("Mode", mode)?;
                }
                Ok(state)
            }
        })?,
    )?;
    actor.set(
        "SetLineWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_line_width", width.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetLineWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_line_width")?
                    .unwrap_or(1.0))
            }
        })?,
    )?;
    actor.set(
        "SetNumVertices",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let count = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(0)
                    .max(0);
                actor.set("__songlua_vertex_count", i64::from(count))?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetVertices",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Table(vertices)) = method_arg(&args, 0).cloned() {
                    actor.set("__songlua_vertex_count", vertices.raw_len() as i64)?;
                    actor.set("__songlua_vertices", vertices)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetNumVertices",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                if let Some(count) = actor.get::<Option<i64>>("__songlua_vertex_count")? {
                    return Ok(count.max(0));
                }
                Ok(actor
                    .get::<Option<Table>>("__songlua_vertices")?
                    .map(|vertices| vertices.raw_len() as i64)
                    .unwrap_or(0))
            }
        })?,
    )?;
    actor.set(
        "cropbottom",
        make_actor_capture_f32_method(lua, actor, "cropbottom", None)?,
    )?;
    actor.set(
        "croptop",
        make_actor_capture_f32_method(lua, actor, "croptop", None)?,
    )?;
    actor.set(
        "fadeleft",
        make_actor_capture_f32_method(lua, actor, "fadeleft", None)?,
    )?;
    actor.set(
        "faderight",
        make_actor_capture_f32_method(lua, actor, "faderight", None)?,
    )?;
    actor.set(
        "fadetop",
        make_actor_capture_f32_method(lua, actor, "fadetop", None)?,
    )?;
    actor.set(
        "fadebottom",
        make_actor_capture_f32_method(lua, actor, "fadebottom", None)?,
    )?;
    actor.set(
        "shadowlength",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec2(lua, &actor, "shadow_len", [value, -value])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowlengthx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let mut len = actor_shadow_len(lua, &actor)?;
                len[0] = value;
                capture_block_set_vec2(lua, &actor, "shadow_len", len)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowlengthy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let mut len = actor_shadow_len(lua, &actor)?;
                len[1] = -value;
                capture_block_set_vec2(lua, &actor, "shadow_len", len)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowcolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec4(lua, &actor, "shadow_color", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "MaskSource",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "mask_source", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "MaskDest",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "mask_dest", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rotationx",
        make_actor_capture_f32_method(lua, actor, "rot_x_deg", Some("rotationx"))?,
    )?;
    actor.set(
        "rotationy",
        make_actor_capture_f32_method(lua, actor, "rot_y_deg", Some("rotationy"))?,
    )?;
    actor.set(
        "rotationz",
        make_actor_capture_f32_method(lua, actor, "rot_z_deg", Some("rotationz"))?,
    )?;
    actor.set(
        "baserotationx",
        make_actor_capture_f32_method(lua, actor, "rot_x_deg", Some("rotationx"))?,
    )?;
    actor.set(
        "baserotationy",
        make_actor_capture_f32_method(lua, actor, "rot_y_deg", Some("rotationy"))?,
    )?;
    actor.set(
        "baserotationz",
        make_actor_capture_f32_method(lua, actor, "rot_z_deg", Some("rotationz"))?,
    )?;
    actor.set(
        "zoomx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "zoomx")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "zoom_x", value)?;
                    actor_update_text_pre_zoom_flags(lua, &actor, true, false)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "zoomy")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "zoom_y", value)?;
                    actor_update_text_pre_zoom_flags(lua, &actor, false, true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomto",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("scaletoclipped", make_actor_set_size_method(lua, actor)?)?;
    actor.set("ScaleToClipped", make_actor_set_size_method(lua, actor)?)?;
    for (name, cover) in [("scaletofit", false), ("scaletocover", true)] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    let Some(left) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(top) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(right) = method_arg(&args, 2).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(bottom) = method_arg(&args, 3).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    scale_actor_to_rect(lua, &actor, [left, top, right, bottom], cover)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "CropTo",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                crop_actor_to(lua, &actor, width, height)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomtowidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (_, height) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomtoheight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(height) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (width, _) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetSize",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("setsize", make_actor_set_size_method(lua, actor)?)?;
    actor.set(
        "SetWidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (_, height) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetHeight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(height) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (width, _) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "z",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "z")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "z", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("addx", make_actor_add_f32_method(lua, actor, "x")?)?;
    actor.set("addy", make_actor_add_f32_method(lua, actor, "y")?)?;
    actor.set("addz", make_actor_add_f32_method(lua, actor, "z")?)?;
    actor.set(
        "addrotationx",
        make_actor_add_f32_method(lua, actor, "rot_x_deg")?,
    )?;
    actor.set(
        "addrotationy",
        make_actor_add_f32_method(lua, actor, "rot_y_deg")?,
    )?;
    actor.set(
        "addrotationz",
        make_actor_add_f32_method(lua, actor, "rot_z_deg")?,
    )?;
    for (name, state_key) in [
        ("skewx", "__songlua_state_skew_x"),
        ("skewy", "__songlua_state_skew_y"),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                let method_name = name.to_string();
                let state_key = state_key.to_string();
                move |lua, args: MultiValue| {
                    record_probe_method_call(lua, &actor, &method_name)?;
                    if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                        actor.set(state_key.as_str(), value)?;
                        let block_key = if method_name == "skewx" {
                            "skew_x"
                        } else {
                            "skew_y"
                        };
                        capture_block_set_f32(lua, &actor, block_key, value)?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "zoomz",
        make_actor_capture_f32_method(lua, actor, "zoom_z", Some("zoomz"))?,
    )?;
    actor.set(
        "blend",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(raw) = args.get(1).cloned().and_then(read_string)
                    && let Some(blend) = parse_overlay_blend_mode(raw.as_str())
                {
                    let block = actor_current_capture_block(lua, &actor)?;
                    let label = match blend {
                        SongLuaOverlayBlendMode::Alpha => "alpha",
                        SongLuaOverlayBlendMode::Add => "add",
                        SongLuaOverlayBlendMode::Multiply => "multiply",
                        SongLuaOverlayBlendMode::Subtract => "subtract",
                    };
                    block.set("blend", label)?;
                    block.set("__songlua_has_changes", true)?;
                    actor.set("__songlua_state_blend", label)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "glow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec4(lua, &actor, "glow", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectmagnitude",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let block = actor_current_capture_block(lua, &actor)?;
                let value = lua.create_table()?;
                value.raw_set(1, args.get(1).cloned().and_then(read_f32).unwrap_or(0.0))?;
                value.raw_set(2, args.get(2).cloned().and_then(read_f32).unwrap_or(0.0))?;
                value.raw_set(3, args.get(3).cloned().and_then(read_f32).unwrap_or(0.0))?;
                block.set("effect_magnitude", value.clone())?;
                block.set("__songlua_has_changes", true)?;
                actor.set("__songlua_state_effect_magnitude", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "customtexturerect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    method_arg(&args, 0)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 1)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 2)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 3)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                ];
                capture_texture_rect(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCustomImageRect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    method_arg(&args, 0)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 1)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 2)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 3)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                ];
                capture_texture_rect(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stretchtexcoords",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let dx = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let dy = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                offset_texture_rect(lua, &actor, dx, dy)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "addimagecoords",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some((width, height)) = actor_image_texture_size(&actor)? else {
                    return Ok(actor.clone());
                };
                if width <= f32::EPSILON || height <= f32::EPSILON {
                    return Ok(actor.clone());
                }
                let dx = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let dy = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                offset_texture_rect(lua, &actor, dx / width, dy / height)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "setstate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(state_index) = method_arg(&args, 0).cloned().and_then(read_u32_value)
                else {
                    return Ok(actor.clone());
                };
                set_actor_sprite_state(lua, &actor, state_index)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetState",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(i64::from(
                    song_lua_valid_sprite_state_index(
                        actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
                    )
                    .unwrap_or(0),
                ))
            }
        })?,
    )?;
    actor.set(
        "SetStateProperties",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(Value::Table(states)) = method_arg(&args, 0) else {
                    return Ok(actor.clone());
                };
                let state_count = states.raw_len();
                if state_count == 0 {
                    return Ok(actor.clone());
                }

                let mut animation_length = 0.0_f32;
                let mut first_frame = None;
                let mut first_delay = None;
                for state in states.sequence_values::<Table>() {
                    let state = state?;
                    if first_frame.is_none() {
                        first_frame = state.get::<Option<u32>>("Frame")?;
                    }
                    let delay = state.get::<Option<f32>>("Delay")?.unwrap_or(0.0).max(0.0);
                    if first_delay.is_none() {
                        first_delay = Some(delay);
                    }
                    animation_length += delay;
                }

                actor.set(
                    "__songlua_sprite_animation_length_seconds",
                    animation_length,
                )?;
                if let Some(delay) = first_delay {
                    capture_block_set_f32(lua, &actor, "sprite_state_delay", delay)?;
                }
                set_actor_sprite_state(lua, &actor, first_frame.unwrap_or(0))?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetAnimationLengthSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                if let Some(value) =
                    actor.get::<Option<f32>>("__songlua_sprite_animation_length_seconds")?
                {
                    return Ok(value);
                }
                let delay = actor
                    .get::<Option<f32>>("__songlua_state_sprite_state_delay")?
                    .unwrap_or(0.1)
                    .max(0.0);
                Ok(actor_sprite_frame_count(&actor)? as f32 * delay)
            }
        })?,
    )?;
    actor.set(
        "SetSecondsIntoAnimation",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                set_actor_seconds_into_animation(lua, &actor, seconds)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetEffectMode",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(mode) = method_arg(&args, 0).cloned().and_then(read_string) {
                    actor.set("__songlua_sprite_effect_mode", mode)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "animate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_animate", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "play",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if actor_type_is(&actor, "Sound")? {
                    capture_block_set_bool(lua, &actor, "sound_play", true)?;
                } else {
                    capture_block_set_bool(lua, &actor, "sprite_animate", true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playforplayer",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if actor_type_is(&actor, "Sound")? {
                    capture_block_set_bool(lua, &actor, "sound_play", true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "pause",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if !actor_type_is(&actor, "Sound")? {
                    capture_block_set_bool(lua, &actor, "sprite_animate", false)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "loop",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_loop", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "sprite_playback_rate", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["SetAllStateDelays", "setallstatedelays"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                        capture_block_set_f32(lua, &actor, "sprite_state_delay", value.max(0.0))?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "texcoordvelocity",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let velocity = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_vec2(lua, &actor, "texcoord_velocity", velocity)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "texturetranslate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let offset = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_vec2(lua, &actor, "texcoord_offset", offset)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "texturewrapping",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "texture_wrapping", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDecodeMovie",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                actor.set("__songlua_state_decode_movie", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDecodeMovie",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_decode_movie(&actor)
        })?,
    )?;
    actor.set(
        "glowshift",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "glowshift",
                    Some(1.0),
                    None,
                    Some([1.0, 1.0, 1.0, 0.2]),
                    Some([1.0, 1.0, 1.0, 0.8]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "glowblink",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "glowshift",
                    Some(1.0),
                    None,
                    Some([1.0, 1.0, 1.0, 0.2]),
                    Some([1.0, 1.0, 1.0, 0.8]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseshift",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseshift",
                    Some(1.0),
                    None,
                    Some([0.0, 0.0, 0.0, 1.0]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseblink",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseshift",
                    Some(1.0),
                    None,
                    Some([0.5, 0.5, 0.5, 0.5]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseramp",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseramp",
                    Some(1.0),
                    None,
                    Some([0.0, 0.0, 0.0, 1.0]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "pulse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "pulse",
                    Some(2.0),
                    Some([0.5, 1.0, 1.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "bob",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "bob",
                    Some(2.0),
                    Some([0.0, 20.0, 0.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "bounce",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "bounce",
                    Some(2.0),
                    Some([0.0, 20.0, 0.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "wag",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "wag",
                    Some(2.0),
                    Some([0.0, 0.0, 20.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "spin",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "spin",
                    None,
                    Some([0.0, 0.0, 180.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectcolor1",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let color = read_color_args(&args).unwrap_or([1.0, 1.0, 1.0, 1.0]);
                capture_block_set_vec4(lua, &actor, "effect_color1", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectcolor2",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let color = read_color_args(&args).unwrap_or([1.0, 1.0, 1.0, 1.0]);
                capture_block_set_vec4(lua, &actor, "effect_color2", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectperiod",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "effect_period", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectoffset",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "effect_offset", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effecttiming",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(a) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(b) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(c) = method_arg(&args, 2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(d) = method_arg(&args, 3).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let timing = if let Some(e) = method_arg(&args, 4).cloned().and_then(read_f32) {
                    [a.max(0.0), b.max(0.0), c.max(0.0), e.max(0.0), d.max(0.0)]
                } else {
                    [a.max(0.0), b.max(0.0), c.max(0.0), 0.0, d.max(0.0)]
                };
                let total = timing.iter().sum::<f32>();
                if total > 0.0 {
                    capture_block_set_vec5(lua, &actor, "effect_timing", timing)?;
                    capture_block_set_f32(lua, &actor, "effect_period", total)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectclock",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(raw) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let Some(clock) = parse_overlay_effect_clock(raw.as_str()) else {
                    return Ok(actor.clone());
                };
                capture_block_set_string(lua, &actor, "effect_clock", effect_clock_label(clock))?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "vibrate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "vibrate", true)?;
                capture_block_set_vec3(lua, &actor, "effect_magnitude", [10.0, 10.0, 10.0])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stopeffect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "vibrate", false)?;
                capture_block_set_bool(lua, &actor, "rainbow", false)?;
                set_actor_effect_mode(lua, &actor, "none")?;
                capture_block_set_vec3(lua, &actor, "effect_magnitude", [0.0, 0.0, 0.0])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "hurrytweening",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(factor) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    prepare_capture_scope_actor(lua, &actor)?;
                    hurry_actor_tweening(&actor, factor)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in [
        "clearzbuffer",
        "Draw",
        "EnableAlphaBuffer",
        "EnableDepthBuffer",
        "EnableFloat",
        "EnablePreserveTexture",
        "Create",
        "SetAmbientLightColor",
        "SetDiffuseLightColor",
        "SetLightDirection",
        "SetSpecularLightColor",
        "SortByDrawOrder",
        "fardistz",
        "backfacecull",
        "StartTransitioningScreen",
        "stop",
        "volume",
        "cullmode",
    ] {
        actor.set(name, make_actor_chain_method(lua, actor)?)?;
    }
    actor.set(
        "position",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                set_actor_seconds_into_animation(lua, &actor, seconds)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zbias",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "z_bias", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDrawByZPosition",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0).map_or(true, truthy);
                capture_block_set_bool(lua, &actor, "draw_by_z_position", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "AddChildFromPath",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                add_actor_child_from_path(lua, &actor, &path)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "load",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if actor_type_is(&actor, "Sound")? {
                    set_actor_sound_file_from_value(&actor, method_arg(&args, 0), true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rainbow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "rainbow", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rainbowscroll",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "rainbow_scroll", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "jitter",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "text_jitter", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "distort",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let amount = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                capture_block_set_f32(lua, &actor, "text_distortion", amount)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "undistort",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_f32(lua, &actor, "text_distortion", 0.0)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "AddAttribute",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                capture_actor_text_attribute(lua, &actor, &args)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ClearAttributes",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                actor.set("__songlua_text_attributes", lua.create_table()?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for (name, corner_mask) in [
        ("diffuseupperleft", 1 << 0),
        ("diffuseupperright", 1 << 1),
        ("diffuselowerright", 1 << 2),
        ("diffuselowerleft", 1 << 3),
        ("diffusetopedge", (1 << 0) | (1 << 1)),
        ("diffuserightedge", (1 << 1) | (1 << 2)),
        ("diffusebottomedge", (1 << 2) | (1 << 3)),
        ("diffuseleftedge", (1 << 0) | (1 << 3)),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    capture_actor_vertex_diffuse(lua, &actor, &args, corner_mask)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetTextureFiltering",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0).map_or(true, truthy);
                capture_block_set_bool(lua, &actor, "texture_filtering", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["zbuffer", "ztest", "zwrite"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    let enabled = method_arg(&args, 0).map_or(true, truthy);
                    capture_block_set_bool(lua, &actor, "depth_test", enabled)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "ztestmode",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .is_none_or(|mode| {
                        let normalized = mode
                            .trim()
                            .trim_start_matches("ZTestMode_")
                            .to_ascii_lowercase();
                        !matches!(normalized.as_str(), "off" | "none" | "false")
                    });
                capture_block_set_bool(lua, &actor, "depth_test", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "finishtweening",
        make_actor_finish_tweening_method(lua, actor)?,
    )?;
    actor.set("stoptweening", make_actor_stop_tweening_method(lua, actor)?)?;
    actor.set(
        "strokecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                actor.set("__songlua_stroke_color", make_color_table(lua, color)?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "getstrokecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let color = read_actor_color_field(&actor, "__songlua_stroke_color")
                    .map_err(mlua::Error::external)?
                    .or_else(|| read_actor_color_field(&actor, "StrokeColor").ok().flatten())
                    .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                make_color_table(lua, color)
            }
        })?,
    )?;
    for name in ["set_mult_attrs_with_diffuse", "mult_attrs_with_diffuse"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    let value = method_arg(&args, 0)
                        .cloned()
                        .and_then(read_boolish)
                        .unwrap_or(true);
                    capture_block_set_bool(lua, &actor, "mult_attrs_with_diffuse", value)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "get_mult_attrs_with_diffuse",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_state_mult_attrs_with_diffuse")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "textglowmode",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(mode) = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .and_then(|mode| parse_overlay_text_glow_mode(mode.as_str()))
                {
                    capture_block_set_string(
                        lua,
                        &actor,
                        "text_glow_mode",
                        text_glow_mode_label(mode),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "settext",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let text = args
                    .get(1)
                    .cloned()
                    .map(lua_text_value)
                    .transpose()?
                    .unwrap_or_default();
                actor.set("Text", text)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "settextf",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                actor.set("Text", lua_format_text(lua, &args)?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetText",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| lua_text_value(actor.get::<Value>("Text")?)
        })?,
    )?;
    for name in ["targetnumber", "SetTargetNumber"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    if let Some(number) = method_arg(&args, 0).cloned().and_then(read_f32) {
                        actor.set("__songlua_target_number", number)?;
                        actor.set("Text", rolling_numbers_text(&actor, number)?)?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "GetTargetNumber",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_target_number")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set("wrapwidthpixels", make_actor_wrap_width_method(lua, actor)?)?;
    actor.set(
        "_wrapwidthpixels",
        make_actor_wrap_width_method(lua, actor)?,
    )?;
    actor.set(
        "vertspacing",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_i32(lua, &actor, "vert_spacing", value.round() as i32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "maxwidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "max_width", value)?;
                capture_block_set_bool(lua, &actor, "max_w_pre_zoom", false)?;
                actor.set("__songlua_text_saw_max_width", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "maxheight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "max_height", value)?;
                capture_block_set_bool(lua, &actor, "max_h_pre_zoom", false)?;
                actor.set("__songlua_text_saw_max_height", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "max_dimension_use_zoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "max_dimension_uses_zoom", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "uppercase",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "uppercase", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_color(lua, &actor, color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffusecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_color(lua, &actor, color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDidTapNoteCallback",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                match args.get(1) {
                    Some(Value::Function(function)) => {
                        actor.set("__songlua_did_tap_note_callback", function.clone())?;
                    }
                    _ => {
                        actor.set("__songlua_did_tap_note_callback", Value::Nil)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    let set_did_tap_note_callback = actor.get::<Function>("SetDidTapNoteCallback")?;
    actor.set("set_did_tap_note_callback", set_did_tap_note_callback)?;
    actor.set(
        "did_tap_note",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let column = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(0);
                let score = method_arg(&args, 1).cloned().unwrap_or(Value::Nil);
                let bright = method_arg(&args, 2).is_some_and(truthy);
                actor.set("__songlua_last_tap_note_column", column)?;
                actor.set("__songlua_last_tap_note_score", score.clone())?;
                actor.set("__songlua_last_tap_note_bright", bright)?;
                if let Some(callback) =
                    actor.get::<Option<Function>>("__songlua_did_tap_note_callback")?
                {
                    let mut callback_args = MultiValue::new();
                    callback_args.push_back(Value::Integer(i64::from(column)));
                    callback_args.push_back(score);
                    callback_args.push_back(Value::Boolean(bright));
                    let _ = callback.call::<Value>(callback_args)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetColumnActors",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let is_note_field = actor
                    .get::<Option<String>>("__songlua_player_child_name")?
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case("NoteField"))
                    || actor
                        .get::<Option<String>>("__songlua_actor_type")?
                        .as_deref()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("NoteField"));
                if !is_note_field {
                    return Ok(Value::Nil);
                }
                Ok(Value::Table(note_field_column_actors(lua, &actor)?))
            }
        })?,
    )?;
    actor.set(
        "get_column_actors",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let Value::Function(method) = actor.get::<Value>("GetColumnActors")? else {
                    return Ok(Value::Nil);
                };
                method.call::<Value>(MultiValue::from_vec(vec![Value::Table(actor.clone())]))
            }
        })?,
    )?;
    actor.set(
        "GetChild",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                let children = actor_named_children(lua, &actor)?;
                if let Some(child) = children.get::<Option<Table>>(name.as_str())? {
                    return Ok(Value::Table(child));
                }
                let child = create_named_child_actor(lua, &actor, &name)?;
                actor_children(lua, &actor)?.set(name.as_str(), child.clone())?;
                Ok(Value::Table(child))
            }
        })?,
    )?;
    actor.set(
        "GetChildAt",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(index) = method_arg(&args, 0).and_then(read_child_index) else {
                    return Ok(Value::Nil);
                };
                actor_child_at(lua, &actor, index)
            }
        })?,
    )?;
    actor.set(
        "RunCommandsOnChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(command) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Function(function) => Some(function),
                    _ => None,
                }) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                for child in actor_direct_children(lua, &actor)? {
                    let _ = match params.clone() {
                        Some(params) => command.call::<Value>((child, params))?,
                        None => command.call::<Value>(child)?,
                    };
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "runcommandsonleaves",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(command) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Function(function) => Some(function),
                    _ => None,
                }) else {
                    return Ok(actor.clone());
                };
                run_command_on_leaves(lua, &actor, &command)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetWrapperState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(index) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Integer(value) => Some(value),
                    Value::Number(value) => Some(value as i64),
                    _ => None,
                }) else {
                    return Ok(Value::Nil);
                };
                let Some(wrapper) = actor_wrappers(lua, &actor)?.get::<Option<Table>>(index)?
                else {
                    return Ok(Value::Nil);
                };
                Ok(Value::Table(wrapper))
            }
        })?,
    )?;
    actor.set(
        "GetNumWrapperStates",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| Ok(actor_wrappers(lua, &actor)?.raw_len() as i64)
        })?,
    )?;
    actor.set(
        "AddWrapperState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let wrapper = create_dummy_actor(lua, "WrapperState")?;
                copy_dummy_actor_tags(&actor, &wrapper)?;
                let wrappers = actor_wrappers(lua, &actor)?;
                let next_index = wrappers.raw_len() + 1;
                wrappers.raw_set(next_index, wrapper.clone())?;
                Ok(wrapper)
            }
        })?,
    )?;
    actor.set(
        "GetChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| actor_named_children(lua, &actor)
        })?,
    )?;
    actor.set(
        "GetNumChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let mut count = 0_i64;
                for pair in actor_named_children(lua, &actor)?.pairs::<Value, Value>() {
                    let _ = pair?;
                    count += 1;
                }
                Ok(count)
            }
        })?,
    )?;
    actor.set(
        "GetX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_x")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetName",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(
                    actor.get::<Option<String>>("Name")?.unwrap_or_default(),
                )?))
            }
        })?,
    )?;
    actor.set(
        "GetY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_y")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetDestX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_x")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetDestY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_y")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetDestZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_z")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetVisible",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_visible")?
                    .unwrap_or(true))
            }
        })?,
    )?;
    actor.set(
        "GetZoom",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_zoom")?
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_base_size(&actor)?.0)
        })?,
    )?;
    actor.set(
        "GetZoomedWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let width = actor_base_size(&actor)?.0;
                Ok(width
                    * actor_zoom_axis(
                        &actor,
                        "__songlua_state_zoom_x",
                        "__songlua_state_basezoom_x",
                    )?)
            }
        })?,
    )?;
    actor.set(
        "GetNumStates",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(i64::from(actor_sprite_frame_count(&actor)?))
        })?,
    )?;
    actor.set(
        "GetHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_base_size(&actor)?.1)
        })?,
    )?;
    actor.set(
        "GetZoomedHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let height = actor_base_size(&actor)?.1;
                Ok(height
                    * actor_zoom_axis(
                        &actor,
                        "__songlua_state_zoom_y",
                        "__songlua_state_basezoom_y",
                    )?)
            }
        })?,
    )?;
    actor.set(
        "GetZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_z")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetRotationX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_x_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetRotationY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_y_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetRotationZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_z_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "getrotation",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let x = actor
                    .get::<Option<f32>>("__songlua_state_rot_x_deg")?
                    .unwrap_or(0.0_f32);
                let y = actor
                    .get::<Option<f32>>("__songlua_state_rot_y_deg")?
                    .unwrap_or(0.0_f32);
                let z = actor
                    .get::<Option<f32>>("__songlua_state_rot_z_deg")?
                    .unwrap_or(0.0_f32);
                Ok((x, y, z))
            }
        })?,
    )?;
    actor.set(
        "GetZoomX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_zoom_x")?
                    .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetZoomY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_zoom_y")?
                    .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetZoomZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_zoom_z")?
                    .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetBaseZoomX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_basezoom_x")?
                    .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetBaseZoomY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_basezoom_y")?
                    .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetBaseZoomZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_basezoom_z")?
                    .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_diffuse(&actor)?[3])
        })?,
    )?;
    actor.set(
        "GetDiffuse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| make_color_table(lua, actor_diffuse(&actor)?)
        })?,
    )?;
    actor.set(
        "GetGlow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| make_color_table(lua, actor_glow(&actor)?)
        })?,
    )?;
    actor.set(
        "GetDiffuseAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_diffuse(&actor)?[3])
        })?,
    )?;
    actor.set(
        "GetHAlign",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_halign(&actor)
        })?,
    )?;
    actor.set(
        "GetVAlign",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_valign(&actor)
        })?,
    )?;
    actor.set(
        "geteffectmagnitude",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let [x, y, z] = actor_effect_magnitude(&actor)?;
                Ok((x, y, z))
            }
        })?,
    )?;
    actor.set(
        "GetSecsIntoEffect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (beat, seconds) = compile_song_runtime_values(lua)?;
                let clock = actor
                    .get::<Option<String>>("__songlua_state_effect_clock")?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock)
                    .unwrap_or(EffectClock::Time);
                Ok(match clock {
                    EffectClock::Time => seconds,
                    EffectClock::Beat => beat,
                })
            }
        })?,
    )?;
    actor.set(
        "GetEffectDelta",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (delta_beat, delta_seconds) = compile_song_runtime_delta_values(lua)?;
                let clock = actor
                    .get::<Option<String>>("__songlua_state_effect_clock")?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock)
                    .unwrap_or(EffectClock::Time);
                Ok(match clock {
                    EffectClock::Time => delta_seconds,
                    EffectClock::Beat => delta_beat,
                })
            }
        })?,
    )?;
    actor.set(
        "GetParent",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<Table>>("__songlua_parent")?
                    .unwrap_or_else(|| actor.clone()))
            }
        })?,
    )?;
    Ok(())
}

fn actor_base_size(actor: &Table) -> mlua::Result<(f32, f32)> {
    if let Some(size) = actor
        .get::<Option<Table>>("__songlua_state_size")?
        .and_then(|value| table_vec2(&value))
    {
        return Ok((size[0].abs(), size[1].abs()));
    }
    if let Some(rect) = actor
        .get::<Option<Table>>("__songlua_state_stretch_rect")?
        .and_then(|value| table_vec4(&value))
    {
        return Ok(((rect[2] - rect[0]).abs(), (rect[3] - rect[1]).abs()));
    }
    if let Some(size) = actor_image_frame_size(actor)? {
        return Ok(size);
    }
    if actor_type_is(actor, "GraphDisplay")? {
        return Ok((
            theme_metric_number("GraphDisplay", "BodyWidth").unwrap_or(300.0),
            theme_metric_number("GraphDisplay", "BodyHeight").unwrap_or(64.0),
        ));
    }
    Ok((1.0, 1.0))
}

fn actor_image_texture_size(actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    if let Some(path) = actor_texture_path(actor)?
        && is_song_lua_image_path(&path)
        && let Ok((width, height)) = image_dimensions(&path)
    {
        return Ok(Some((width as f32, height as f32)));
    }
    Ok(None)
}

fn actor_image_frame_size(actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    if let Some((mut width, mut height)) = actor_image_texture_size(actor)? {
        if actor
            .get::<Option<bool>>("__songlua_state_sprite_animate")?
            .unwrap_or(false)
            || song_lua_valid_sprite_state_index(
                actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
            )
            .is_some()
        {
            if let Some(path) = actor_texture_path(actor)? {
                let (cols, rows) =
                    crate::assets::parse_sprite_sheet_dims(path.to_string_lossy().as_ref());
                width /= cols.max(1) as f32;
                height /= rows.max(1) as f32;
            }
        }
        return Ok(Some((width, height)));
    }
    Ok(None)
}

fn actor_crop_source_size(lua: &Lua, actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    if let Some(size) = actor_image_frame_size(actor)? {
        return Ok(Some(size));
    }
    let Some(path) = actor_texture_path(actor)? else {
        return Ok(None);
    };
    if is_song_lua_video_path(&path) {
        return song_lua_screen_size(lua).map(Some);
    }
    Ok(None)
}

fn crop_actor_to(lua: &Lua, actor: &Table, width: f32, height: f32) -> mlua::Result<()> {
    let target = [width, height];
    if !target.iter().all(|value| value.is_finite() && *value > 0.0) {
        return Ok(());
    }
    let Some((source_width, source_height)) = actor_crop_source_size(lua, actor)? else {
        return Ok(());
    };
    let Some(rect) = crop_texture_rect([source_width, source_height], target) else {
        return Ok(());
    };
    capture_block_set_size(lua, actor, target)?;
    capture_block_set_u32(
        lua,
        actor,
        "sprite_state_index",
        SONG_LUA_SPRITE_STATE_CLEAR,
    )?;
    capture_block_set_vec4(lua, actor, "custom_texture_rect", rect)?;
    capture_block_set_f32(lua, actor, "zoom", 1.0)?;
    capture_block_set_zoom_axes(lua, actor, 1.0, "zoom_x", "zoom_y", "zoom_z")?;
    Ok(())
}

fn crop_texture_rect(source: [f32; 2], target: [f32; 2]) -> Option<[f32; 4]> {
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

fn actor_zoom_axis(actor: &Table, zoom_key: &str, basezoom_key: &str) -> mlua::Result<f32> {
    let zoom = actor
        .get::<Option<f32>>(zoom_key)?
        .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
        .unwrap_or(1.0);
    let basezoom = actor
        .get::<Option<f32>>(basezoom_key)?
        .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
        .unwrap_or(1.0);
    Ok(basezoom * zoom)
}

fn actor_texture_is_video(actor: &Table) -> mlua::Result<bool> {
    if let Some(path) = actor_texture_path(actor)? {
        return Ok(is_song_lua_video_path(&path));
    }
    Ok(actor
        .get::<Option<String>>("Texture")?
        .is_some_and(|texture| is_song_lua_video_path(Path::new(texture.trim()))))
}

fn set_actor_decode_movie_for_texture(actor: &Table) -> mlua::Result<()> {
    actor.set(
        "__songlua_state_decode_movie",
        actor_texture_is_video(actor)?,
    )
}

fn set_actor_texture_from_path(actor: &Table, path: &str) -> mlua::Result<bool> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(false);
    }
    actor.set("Texture", path.to_string())?;
    actor.set("__songlua_aft_capture_name", Value::Nil)?;
    set_actor_decode_movie_for_texture(actor)?;
    Ok(true)
}

fn set_actor_texture_from_path_methods(
    actor: &Table,
    value: Option<&Value>,
    method_names: &[&str],
) -> mlua::Result<bool> {
    let Some(Value::Table(source)) = value else {
        return Ok(false);
    };
    for method_name in method_names {
        if let Value::String(path) = call_table_method(source, method_name)? {
            if set_actor_texture_from_path(actor, &path.to_str()?)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn set_actor_texture_from_path_methods_or_fallback(
    actor: &Table,
    value: Option<&Value>,
    method_names: &[&str],
    fallback_path: &str,
) -> mlua::Result<()> {
    if !set_actor_texture_from_path_methods(actor, value, method_names)? {
        set_actor_texture_from_path(actor, fallback_path)?;
    }
    Ok(())
}

fn banner_sort_order_path(sort_order: &str) -> Option<String> {
    let short = sort_order
        .trim()
        .split_once('_')
        .map(|(_, short)| short)
        .unwrap_or(sort_order.trim());
    let short = short
        .strip_suffix("_P1")
        .or_else(|| short.strip_suffix("_P2"))
        .unwrap_or(short);
    match short {
        "Group" => None,
        "" | "Invalid" => Some(theme_path("G", "Common", "fallback banner")),
        sort => Some(theme_path("G", "Banner", sort)),
    }
}

fn set_actor_texture_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    match value {
        Some(Value::String(texture)) => {
            actor.set("Texture", texture.to_str()?.to_string())?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            set_actor_decode_movie_for_texture(actor)?;
        }
        Some(Value::Table(texture)) => {
            if let Some(capture_name) =
                texture.get::<Option<String>>("__songlua_aft_capture_name")?
            {
                actor.set("__songlua_aft_capture_name", capture_name)?;
                actor.set("Texture", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            } else if let Some(texture_path) = texture
                .get::<Option<String>>("__songlua_texture_path")?
                .filter(|path| !path.is_empty())
            {
                actor.set("Texture", texture_path)?;
                actor.set("__songlua_aft_capture_name", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            }
        }
        Some(Value::Nil) | None if clear_on_nil => {
            actor.set("Texture", Value::Nil)?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            actor.set("__songlua_state_decode_movie", false)?;
        }
        _ => {}
    }
    Ok(())
}

fn set_actor_sound_file_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    match value {
        Some(Value::String(file)) => actor.set("File", file.to_str()?.to_string())?,
        Some(Value::Nil) | None if clear_on_nil => actor.set("File", Value::Nil)?,
        _ => {}
    }
    Ok(())
}

fn actor_decode_movie(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<bool>>("__songlua_state_decode_movie")?
        .unwrap_or(actor_texture_is_video(actor)?))
}

fn create_texture_proxy(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let texture = lua.create_table()?;
    let (screen_width, screen_height) = song_lua_screen_size(lua)?;
    let frame_count = actor_sprite_frame_count(actor)?;
    if actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
    {
        if let Some(capture_name) = actor_aft_capture_name(actor)? {
            texture.set("__songlua_aft_capture_name", capture_name)?;
        }
        install_texture_proxy_methods(
            lua,
            &texture,
            actor,
            String::new(),
            screen_width,
            screen_height,
            screen_width,
            screen_height,
            frame_count,
        )?;
        return Ok(texture);
    }

    let raw_texture = actor.get::<Option<String>>("Texture")?.unwrap_or_default();
    let resolved = actor_texture_path(actor)?;
    let path = resolved
        .as_deref()
        .map(file_path_string)
        .unwrap_or_else(|| raw_texture.clone());
    let (source_width, source_height) = texture_source_size(
        lua,
        actor,
        resolved
            .as_deref()
            .unwrap_or_else(|| Path::new(&raw_texture)),
    )?;
    install_texture_proxy_methods(
        lua,
        &texture,
        actor,
        path,
        source_width,
        source_height,
        source_width,
        source_height,
        frame_count,
    )?;
    Ok(texture)
}

fn install_texture_proxy_methods(
    lua: &Lua,
    texture: &Table,
    actor: &Table,
    path: String,
    source_width: f32,
    source_height: f32,
    texture_width: f32,
    texture_height: f32,
    frame_count: u32,
) -> mlua::Result<()> {
    texture.set("__songlua_texture_path", path.clone())?;
    texture.set(
        "GetPath",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&path)?))
        })?,
    )?;
    texture.set(
        "GetSourceWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(source_width))?,
    )?;
    texture.set(
        "GetSourceHeight",
        lua.create_function(move |_, _args: MultiValue| Ok(source_height))?,
    )?;
    texture.set(
        "GetTextureWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(texture_width))?,
    )?;
    texture.set(
        "GetTextureHeight",
        lua.create_function(move |_, _args: MultiValue| Ok(texture_height))?,
    )?;
    texture.set(
        "GetNumFrames",
        lua.create_function(move |_, _args: MultiValue| Ok(i64::from(frame_count)))?,
    )?;
    texture.set(
        "loop",
        lua.create_function({
            let actor = actor.clone();
            let texture = texture.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_loop", value)?;
                Ok(Value::Table(texture.clone()))
            }
        })?,
    )?;
    texture.set(
        "rate",
        lua.create_function({
            let actor = actor.clone();
            let texture = texture.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "sprite_playback_rate", value)?;
                }
                Ok(Value::Table(texture.clone()))
            }
        })?,
    )?;
    Ok(())
}

fn texture_source_size(lua: &Lua, actor: &Table, path: &Path) -> mlua::Result<(f32, f32)> {
    if is_song_lua_image_path(path)
        && let Ok((width, height)) = image_dimensions(path)
    {
        return Ok((width as f32, height as f32));
    }
    if is_song_lua_video_path(path) {
        return song_lua_screen_size(lua);
    }
    actor_base_size(actor)
}

fn song_lua_screen_size(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let globals = lua.globals();
    let width = globals.get::<Option<i32>>("SCREEN_WIDTH")?.unwrap_or(640) as f32;
    let height = globals.get::<Option<i32>>("SCREEN_HEIGHT")?.unwrap_or(480) as f32;
    Ok((width, height))
}

fn song_lua_screen_center(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let globals = lua.globals();
    let center_x = globals
        .get::<Option<f32>>("SCREEN_CENTER_X")?
        .unwrap_or(song_lua_screen_size(lua)?.0 * 0.5);
    let center_y = globals
        .get::<Option<f32>>("SCREEN_CENTER_Y")?
        .unwrap_or(song_lua_screen_size(lua)?.1 * 0.5);
    Ok((center_x, center_y))
}

fn actor_texture_path(actor: &Table) -> mlua::Result<Option<PathBuf>> {
    let Some(texture) = actor.get::<Option<String>>("Texture")? else {
        return Ok(None);
    };
    let texture = texture.trim();
    if texture.is_empty() {
        return Ok(None);
    }
    let raw = Path::new(texture);
    if raw.is_absolute() && raw.exists() {
        return Ok(Some(raw.to_path_buf()));
    }
    if let Some(script_dir) = actor.get::<Option<String>>("__songlua_script_dir")? {
        let candidate = Path::new(&script_dir).join(texture);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    if let Some(song_dir) = actor.get::<Option<String>>("__songlua_song_dir")? {
        let candidate = Path::new(&song_dir).join(texture);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn install_actor_metatable(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let mt = lua.create_table()?;
    let actor_clone = actor.clone();
    mt.set(
        "__concat",
        lua.create_function(move |lua, (_lhs, rhs): (Value, Value)| {
            if let Value::Table(rhs) = rhs {
                merge_actor_concat(lua, &actor_clone, &rhs)?;
            }
            Ok(actor_clone.clone())
        })?,
    )?;
    let actor_clone = actor.clone();
    mt.set(
        "__tostring",
        lua.create_function(move |_, _self: Value| Ok(actor_debug_label(&actor_clone)))?,
    )?;
    let _ = actor.set_metatable(Some(mt));
    Ok(())
}

fn merge_actor_concat(_lua: &Lua, actor: &Table, rhs: &Table) -> mlua::Result<()> {
    let next_index = actor.raw_len() + 1;
    let mut append_index = next_index;
    for value in rhs.sequence_values::<Value>() {
        actor.raw_set(append_index, value?)?;
        append_index += 1;
    }
    for pair in rhs.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let is_sequence_key = match key {
            Value::Integer(index) => index >= 1,
            Value::Number(index) => index.is_finite() && index >= 1.0 && index.fract() == 0.0,
            _ => false,
        };
        if is_sequence_key {
            continue;
        }
        if matches!(
            &key,
            Value::String(text)
                if text.to_str().ok().is_some_and(|name|
                    name == "__songlua_actor_type"
                        || name == "__songlua_script_dir"
                        || name == "__songlua_song_dir")
        ) {
            continue;
        }
        actor.set(key, value)?;
    }
    Ok(())
}

fn add_actor_child_from_path(lua: &Lua, actor: &Table, path: &str) -> mlua::Result<()> {
    let Some(song_dir) =
        actor
            .get::<Option<String>>("__songlua_song_dir")?
            .or(lua.globals().get::<Option<String>>("__songlua_song_dir")?)
    else {
        return Ok(());
    };
    let Ok(Value::Table(child)) = load_actor_path(lua, Path::new(&song_dir), path) else {
        return Ok(());
    };
    child.set("__songlua_parent", actor.clone())?;
    if !push_sequence_child_once(actor, child.clone())? {
        return Ok(());
    }
    run_added_actor_child_commands(lua, actor, &child)
}

fn run_added_actor_child_commands(lua: &Lua, parent: &Table, child: &Table) -> mlua::Result<()> {
    if parent
        .get::<Option<bool>>("__songlua_init_commands_ran")?
        .unwrap_or(false)
    {
        run_actor_init_commands_for_table(lua, child)?;
    }
    let startup_already_needs_child = parent
        .get::<Option<bool>>("__songlua_startup_commands_ran")?
        .unwrap_or(false)
        || parent
            .get::<Option<bool>>("__songlua_startup_children_walked")?
            .unwrap_or(false);
    if startup_already_needs_child {
        run_actor_startup_commands_for_table(lua, child)?;
    }
    Ok(())
}

fn make_actor_chain_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |_, _args: MultiValue| Ok(actor.clone()))
}

fn make_actor_stop_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, _args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        flush_actor_capture(&actor)?;
        reset_actor_capture(lua, &actor)?;
        Ok(actor.clone())
    })
}

fn make_actor_finish_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, _args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        finish_actor_tweening(lua, &actor)?;
        Ok(actor.clone())
    })
}

fn make_actor_wrap_width_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let wrap = value as i32;
        if wrap >= 0 {
            capture_block_set_i32(lua, &actor, "wrap_width_pixels", wrap)?;
        }
        Ok(actor.clone())
    })
}

fn make_actor_set_size_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let Some(height) = method_arg(&args, 1).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        capture_block_set_size(lua, &actor, [width, height])?;
        Ok(actor.clone())
    })
}

fn run_command_on_leaves(lua: &Lua, actor: &Table, command: &Function) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_command_on_leaves(lua, &child, command)?;
    }
    if !saw_child {
        let _ = command.call::<Value>(actor.clone())?;
    }
    Ok(())
}

fn run_named_command_on_leaves(
    lua: &Lua,
    actor: &Table,
    command: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_named_command_on_leaves(lua, &child, command, params.clone())?;
    }
    if !saw_child {
        run_actor_named_command_with_drain_and_params(lua, actor, command, true, params)?;
    }
    Ok(())
}

fn run_named_command_on_children_recursively(
    lua: &Lua,
    actor: &Table,
    command: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    for child in actor_direct_children(lua, actor)? {
        run_actor_named_command_with_drain_and_params(lua, &child, command, true, params.clone())?;
        run_named_command_on_children_recursively(lua, &child, command, params.clone())?;
    }
    Ok(())
}

fn remove_actor_child(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    actor_children(lua, actor)?.set(name, Value::Nil)?;
    let mut write = 1;
    for read in 1..=actor.raw_len() {
        let value = actor.raw_get::<Value>(read)?;
        let remove = match &value {
            Value::Table(child) => child
                .get::<Option<String>>("Name")?
                .is_some_and(|child_name| child_name == name),
            _ => false,
        };
        if remove {
            continue;
        }
        if write != read {
            actor.raw_set(write, value)?;
        }
        write += 1;
    }
    for index in write..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    Ok(())
}

fn remove_all_actor_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    for index in 1..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    let children = actor_children(lua, actor)?;
    let mut keys = Vec::new();
    for pair in children.pairs::<Value, Value>() {
        let (key, _) = pair?;
        keys.push(key);
    }
    for key in keys {
        children.set(key, Value::Nil)?;
    }
    Ok(())
}

fn make_actor_capture_f32_method(
    lua: &Lua,
    actor: &Table,
    key: &'static str,
    probe_name: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        if let Some(probe_name) = probe_name {
            record_probe_method_call(lua, &actor, probe_name)?;
        }
        if let Some(value) = args.get(1).cloned().and_then(read_f32) {
            capture_block_set_f32(lua, &actor, key, value)?;
        }
        Ok(actor.clone())
    })
}

fn make_actor_add_f32_method(
    lua: &Lua,
    actor: &Table,
    key: &'static str,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(delta) = args.get(1).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let block = actor_current_capture_block(lua, &actor)?;
        let current = block
            .get::<Option<f32>>(key)?
            .or(actor.get::<Option<f32>>(format!("__songlua_state_{key}"))?)
            .unwrap_or(0.0);
        capture_block_set_f32(lua, &actor, key, current + delta)?;
        Ok(actor.clone())
    })
}

fn make_actor_tween_method(
    lua: &Lua,
    actor: &Table,
    easing: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        flush_actor_capture(&actor)?;
        let cursor = actor
            .get::<Option<f32>>("__songlua_capture_cursor")?
            .unwrap_or(0.0);
        let duration = args
            .get(1)
            .cloned()
            .and_then(read_f32)
            .unwrap_or(0.0)
            .max(0.0);
        actor.set("__songlua_capture_duration", duration)?;
        actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
        actor.set("__songlua_capture_easing", easing)?;
        actor.set("__songlua_capture_opt1", Value::Nil)?;
        actor.set("__songlua_capture_opt2", Value::Nil)?;
        Ok(actor.clone())
    })
}

fn read_color_args(args: &MultiValue) -> Option<[f32; 4]> {
    if let Some(color) = method_arg(args, 0).cloned().and_then(read_color_value) {
        return Some(color);
    }
    let r = method_arg(args, 0).cloned().and_then(read_f32)?;
    let g = method_arg(args, 1).cloned().and_then(read_f32)?;
    let b = method_arg(args, 2).cloned().and_then(read_f32)?;
    let a = method_arg(args, 3)
        .cloned()
        .and_then(read_f32)
        .unwrap_or(1.0);
    Some([r, g, b, a])
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

fn record_probe_method_call(lua: &Lua, actor: &Table, method_name: &str) -> mlua::Result<()> {
    record_probe_actor_call(lua, actor)?;
    let globals = lua.globals();
    let Some(calls) = globals.get::<Option<Table>>(SONG_LUA_PROBE_METHODS_KEY)? else {
        return Ok(());
    };
    calls.raw_set(
        calls.raw_len() + 1,
        format!("{}.{}", probe_target_kind(actor)?, method_name),
    )?;
    Ok(())
}

fn record_probe_actor_call(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(actors) = globals.get::<Option<Table>>(SONG_LUA_PROBE_ACTORS_KEY)? else {
        return Ok(());
    };
    let Some(seen) = globals.get::<Option<Table>>(SONG_LUA_PROBE_ACTOR_SET_KEY)? else {
        return Ok(());
    };
    if seen
        .raw_get::<Option<bool>>(actor.clone())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    seen.raw_set(actor.clone(), true)?;
    actors.raw_set(actors.raw_len() + 1, actor.clone())?;
    Ok(())
}

fn prepare_capture_scope_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(actors) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_ACTORS_KEY)? else {
        return Ok(());
    };
    let Some(seen) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_ACTOR_SET_KEY)? else {
        return Ok(());
    };
    let Some(snapshots) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_SNAPSHOTS_KEY)? else {
        return Ok(());
    };
    if seen
        .raw_get::<Option<bool>>(actor.clone())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    seen.raw_set(actor.clone(), true)?;
    actors.raw_set(actors.raw_len() + 1, actor.clone())?;

    let snapshot = lua.create_table()?;
    snapshot.raw_set(1, actor.clone())?;
    snapshot.raw_set(2, snapshot_actor_semantic_state_table(lua, actor)?)?;
    snapshots.raw_set(snapshots.raw_len() + 1, snapshot)?;

    reset_actor_capture(lua, actor)?;
    Ok(())
}

fn probe_target_kind(actor: &Table) -> mlua::Result<&'static str> {
    let Some(_player_index) = actor.get::<Option<i64>>("__songlua_player_index")? else {
        return Ok("overlay");
    };
    Ok(
        match actor
            .get::<Option<String>>("__songlua_player_child_name")?
            .as_deref()
        {
            None => "player",
            Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
            Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
            Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
            _ => "player-child",
        },
    )
}

fn probe_function_ease_target(
    lua: &Lua,
    function: &Function,
) -> mlua::Result<(Option<SongLuaEaseTarget>, Vec<String>, Vec<usize>)> {
    let globals = lua.globals();
    let previous_time = compile_song_runtime_values(lua)?;
    set_compile_song_runtime_beat(lua, 0.0)?;
    let previous_methods = globals.get::<Value>(SONG_LUA_PROBE_METHODS_KEY)?;
    let previous_actors = globals.get::<Value>(SONG_LUA_PROBE_ACTORS_KEY)?;
    let previous_actor_set = globals.get::<Value>(SONG_LUA_PROBE_ACTOR_SET_KEY)?;
    let calls = lua.create_table()?;
    let actors = lua.create_table()?;
    globals.set(SONG_LUA_PROBE_METHODS_KEY, calls.clone())?;
    globals.set(SONG_LUA_PROBE_ACTORS_KEY, actors.clone())?;
    globals.set(SONG_LUA_PROBE_ACTOR_SET_KEY, lua.create_table()?)?;
    let result = function.call::<Value>(1.0_f32);
    let methods = probe_call_names(&calls)?;
    let actor_ptrs = probe_actor_pointers(&actors)?;
    let classify = classify_function_ease_probe(&calls);
    globals.set(SONG_LUA_PROBE_METHODS_KEY, previous_methods)?;
    globals.set(SONG_LUA_PROBE_ACTORS_KEY, previous_actors)?;
    globals.set(SONG_LUA_PROBE_ACTOR_SET_KEY, previous_actor_set)?;
    set_compile_song_runtime_values(lua, previous_time.0, previous_time.1)?;
    match result {
        Ok(_) => Ok((classify?, methods, actor_ptrs)),
        Err(_) => Ok((None, methods, actor_ptrs)),
    }
}

fn classify_function_ease_probe(calls: &Table) -> mlua::Result<Option<SongLuaEaseTarget>> {
    let mut saw_x = false;
    let mut saw_y = false;
    let mut saw_z = false;
    let mut saw_rotation_x = false;
    let mut saw_rotation_z = false;
    let mut saw_rotation_y = false;
    let mut saw_skew_x = false;
    let mut saw_skew_y = false;
    let mut saw_zoom = false;
    let mut saw_zoom_x = false;
    let mut saw_zoom_y = false;
    let mut saw_zoom_z = false;
    for value in calls.sequence_values::<String>() {
        let value = value?;
        let (target_kind, method_name) =
            value.split_once('.').unwrap_or(("player", value.as_str()));
        if !matches!(target_kind, "player" | "notefield" | "overlay") {
            return Ok(None);
        }
        match method_name {
            "x" if target_kind == "player" => saw_x = true,
            "y" if target_kind == "player" => saw_y = true,
            "z" if target_kind != "overlay" => saw_z = true,
            "rotationx" if target_kind != "overlay" => saw_rotation_x = true,
            "rotationz" if target_kind != "overlay" => saw_rotation_z = true,
            "rotationy" if target_kind != "overlay" => saw_rotation_y = true,
            "skewx" => saw_skew_x = true,
            "skewy" => saw_skew_y = true,
            "zoom" if target_kind != "overlay" => saw_zoom = true,
            "zoomx" if target_kind != "overlay" => saw_zoom_x = true,
            "zoomy" if target_kind != "overlay" => saw_zoom_y = true,
            "zoomz" if target_kind != "overlay" => saw_zoom_z = true,
            _ => return Ok(None),
        }
    }
    Ok(
        match (
            saw_x,
            saw_y,
            saw_z,
            saw_rotation_x,
            saw_rotation_z,
            saw_rotation_y,
            saw_skew_x,
            saw_skew_y,
            saw_zoom,
            saw_zoom_x,
            saw_zoom_y,
            saw_zoom_z,
        ) {
            (true, false, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerX)
            }
            (false, true, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerY)
            }
            (false, false, true, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZ)
            }
            (false, false, false, true, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationX)
            }
            (false, false, false, false, true, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationZ)
            }
            (false, false, false, false, false, true, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationY)
            }
            (false, false, false, false, false, false, true, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewX)
            }
            (false, false, false, false, false, false, false, true, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewY)
            }
            (false, false, false, false, false, false, false, false, true, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoom)
            }
            (false, false, false, false, false, false, false, false, false, true, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoomX)
            }
            (false, false, false, false, false, false, false, false, false, false, true, false) => {
                Some(SongLuaEaseTarget::PlayerZoomY)
            }
            (false, false, false, false, false, false, false, false, false, false, false, true) => {
                Some(SongLuaEaseTarget::PlayerZoomZ)
            }
            _ => None,
        },
    )
}

fn probe_call_names(calls: &Table) -> mlua::Result<Vec<String>> {
    let mut out = Vec::new();
    for value in calls.sequence_values::<String>() {
        out.push(value?);
    }
    Ok(out)
}

fn probe_actor_pointers(actors: &Table) -> mlua::Result<Vec<usize>> {
    let mut out = Vec::new();
    for value in actors.sequence_values::<Table>() {
        out.push(value?.to_pointer() as usize);
    }
    Ok(out)
}

fn read_tracked_compile_actors(lua: &Lua) -> Result<Vec<TrackedCompileActor>, String> {
    let globals = lua.globals();
    let top_screen = globals
        .get::<Table>("__songlua_top_screen")
        .map_err(|err| err.to_string())?;
    let children = actor_children(lua, &top_screen).map_err(|err| err.to_string())?;
    let song_foreground = if let Some(actor) = children
        .get::<Option<Table>>("SongForeground")
        .map_err(|err| err.to_string())?
    {
        actor
    } else {
        let actor = create_named_child_actor(lua, &top_screen, "SongForeground")
            .map_err(|err| err.to_string())?;
        children
            .set("SongForeground", actor.clone())
            .map_err(|err| err.to_string())?;
        actor
    };
    Ok(vec![
        tracked_compile_actor(
            globals
                .get::<Table>("__songlua_top_screen_player_1")
                .map_err(|err| err.to_string())?,
            TrackedCompileActorTarget::Player(0),
        )?,
        tracked_compile_actor(
            globals
                .get::<Table>("__songlua_top_screen_player_2")
                .map_err(|err| err.to_string())?,
            TrackedCompileActorTarget::Player(1),
        )?,
        tracked_compile_actor(song_foreground, TrackedCompileActorTarget::SongForeground)?,
    ])
}

fn tracked_compile_actor(
    table: Table,
    target: TrackedCompileActorTarget,
) -> Result<TrackedCompileActor, String> {
    Ok(TrackedCompileActor {
        actor: SongLuaCapturedActor {
            initial_state: actor_overlay_initial_state(&table)?,
            message_commands: Vec::new(),
        },
        table,
        target,
    })
}

fn reset_overlay_capture_tables(lua: &Lua, overlays: &[OverlayCompileActor]) -> Result<(), String> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    reset_overlay_capture_tables_for_indices(lua, overlays, &indices)
}

fn reset_overlay_capture_tables_for_indices(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    indices: &[usize],
) -> Result<(), String> {
    for &index in indices {
        let Some(overlay) = overlays.get(index) else {
            continue;
        };
        reset_actor_capture(lua, &overlay.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn reset_tracked_capture_tables(
    lua: &Lua,
    tracked_actors: &[TrackedCompileActor],
) -> Result<(), String> {
    let indices: Vec<_> = (0..tracked_actors.len()).collect();
    reset_tracked_capture_tables_for_indices(lua, tracked_actors, &indices)
}

fn reset_tracked_capture_tables_for_indices(
    lua: &Lua,
    tracked_actors: &[TrackedCompileActor],
    indices: &[usize],
) -> Result<(), String> {
    for &index in indices {
        let Some(actor) = tracked_actors.get(index) else {
            continue;
        };
        reset_actor_capture(lua, &actor.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn collect_overlay_capture_blocks_for_indices(
    overlays: &[OverlayCompileActor],
    indices: &[usize],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for &idx in indices {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        flush_actor_capture(&overlay.table).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(&overlay.table)?;
        if !blocks.is_empty() {
            out.push((idx, blocks));
        }
    }
    Ok(out)
}

fn collect_tracked_capture_blocks_for_indices(
    tracked_actors: &[TrackedCompileActor],
    indices: &[usize],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for &idx in indices {
        let Some(actor) = tracked_actors.get(idx) else {
            continue;
        };
        flush_actor_capture(&actor.table).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(&actor.table)?;
        if !blocks.is_empty() {
            out.push((idx, blocks));
        }
    }
    Ok(out)
}

struct SongLuaActionCaptureScope {
    actors: Table,
    snapshots: Table,
    previous_actors: Value,
    previous_actor_set: Value,
    previous_snapshots: Value,
}

fn begin_action_capture_scope(lua: &Lua) -> mlua::Result<SongLuaActionCaptureScope> {
    let globals = lua.globals();
    let previous_actors = globals.get::<Value>(SONG_LUA_CAPTURE_ACTORS_KEY)?;
    let previous_actor_set = globals.get::<Value>(SONG_LUA_CAPTURE_ACTOR_SET_KEY)?;
    let previous_snapshots = globals.get::<Value>(SONG_LUA_CAPTURE_SNAPSHOTS_KEY)?;
    let actors = lua.create_table()?;
    let snapshots = lua.create_table()?;
    globals.set(SONG_LUA_CAPTURE_ACTORS_KEY, actors.clone())?;
    globals.set(SONG_LUA_CAPTURE_ACTOR_SET_KEY, lua.create_table()?)?;
    globals.set(SONG_LUA_CAPTURE_SNAPSHOTS_KEY, snapshots.clone())?;
    Ok(SongLuaActionCaptureScope {
        actors,
        snapshots,
        previous_actors,
        previous_actor_set,
        previous_snapshots,
    })
}

fn restore_action_capture_scope(lua: &Lua, scope: SongLuaActionCaptureScope) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(SONG_LUA_CAPTURE_ACTORS_KEY, scope.previous_actors)?;
    globals.set(SONG_LUA_CAPTURE_ACTOR_SET_KEY, scope.previous_actor_set)?;
    globals.set(SONG_LUA_CAPTURE_SNAPSHOTS_KEY, scope.previous_snapshots)?;
    Ok(())
}

fn capture_scope_actor_pointers(actors: &Table) -> mlua::Result<HashSet<usize>> {
    let mut out = HashSet::with_capacity(actors.raw_len());
    for actor in actors.sequence_values::<Table>() {
        out.insert(actor?.to_pointer() as usize);
    }
    Ok(out)
}

fn capture_scope_actor_tables(actors: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::with_capacity(actors.raw_len());
    for actor in actors.sequence_values::<Table>() {
        out.push(actor?);
    }
    Ok(out)
}

fn capture_scope_snapshots(snapshots: &Table) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    let mut out = Vec::with_capacity(snapshots.raw_len());
    for snapshot in snapshots.sequence_values::<Table>() {
        let snapshot = snapshot?;
        let actor = snapshot.raw_get(1)?;
        let state = read_actor_semantic_state_table(&snapshot.raw_get(2)?)?;
        out.push((actor, state));
    }
    Ok(out)
}

fn reset_actor_capture_tables(lua: &Lua, actors: &[Table]) -> Result<(), String> {
    for actor in actors {
        reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn overlay_indices_for_actor_pointers(
    overlays: &[OverlayCompileActor],
    actor_ptrs: &HashSet<usize>,
) -> Vec<usize> {
    overlays
        .iter()
        .enumerate()
        .filter(|(_, overlay)| actor_ptrs.contains(&(overlay.table.to_pointer() as usize)))
        .map(|(index, _)| index)
        .collect()
}

fn tracked_indices_for_actor_pointers(
    tracked_actors: &[TrackedCompileActor],
    actor_ptrs: &HashSet<usize>,
) -> Vec<usize> {
    tracked_actors
        .iter()
        .enumerate()
        .filter(|(_, actor)| actor_ptrs.contains(&(actor.table.to_pointer() as usize)))
        .map(|(index, _)| index)
        .collect()
}

fn capture_overlay_function_blocks(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    overlay_indices: &[usize],
    function: &Function,
    arg: Option<f32>,
    song_beat: Option<f32>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    if let Some(song_beat) = song_beat {
        set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    }
    let snapshot_tables: Vec<_> = overlay_indices
        .iter()
        .filter_map(|&index| overlays.get(index))
        .map(|overlay| overlay.table.clone())
        .collect();
    let state_snapshot =
        snapshot_actors_semantic_state(lua, &snapshot_tables).map_err(|err| err.to_string())?;
    reset_overlay_capture_tables_for_indices(lua, overlays, overlay_indices)?;
    let result = match arg {
        Some(value) => function.call::<Value>(value),
        None => function.call::<Value>(()),
    };
    let blocks = collect_overlay_capture_blocks_for_indices(overlays, overlay_indices);
    reset_overlay_capture_tables_for_indices(lua, overlays, overlay_indices)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let blocks = blocks?;
    result.map_err(|err| err.to_string())?;
    Ok(blocks)
}

fn capture_function_action_blocks(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
    function: &Function,
    beat: f32,
) -> Result<
    (
        Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
        Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
        Vec<(String, bool)>,
        bool,
    ),
    String,
> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    let globals = lua.globals();
    let previous_broadcasts = globals
        .get::<Value>(SONG_LUA_BROADCASTS_KEY)
        .map_err(|err| err.to_string())?;
    let broadcast_table = lua.create_table().map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_BROADCASTS_KEY, broadcast_table.clone())
        .map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    let capture_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
    let result = function.call::<Value>(());
    let touched_actors =
        capture_scope_actor_tables(&capture_scope.actors).map_err(|err| err.to_string())?;
    let actor_ptrs =
        capture_scope_actor_pointers(&capture_scope.actors).map_err(|err| err.to_string())?;
    let state_snapshot =
        capture_scope_snapshots(&capture_scope.snapshots).map_err(|err| err.to_string())?;
    restore_action_capture_scope(lua, capture_scope).map_err(|err| err.to_string())?;
    let overlay_indices = overlay_indices_for_actor_pointers(overlays, &actor_ptrs);
    let tracked_indices = tracked_indices_for_actor_pointers(tracked_actors, &actor_ptrs);
    let overlay_blocks = collect_overlay_capture_blocks_for_indices(overlays, &overlay_indices);
    let tracked_blocks =
        collect_tracked_capture_blocks_for_indices(tracked_actors, &tracked_indices);
    let broadcasts = read_song_lua_broadcasts(&broadcast_table).map_err(|err| err.to_string());
    reset_actor_capture_tables(lua, &touched_actors)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_BROADCASTS_KEY, previous_broadcasts)
        .map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let overlay_blocks = overlay_blocks?;
    let tracked_blocks = tracked_blocks?;
    let broadcasts = broadcasts?;
    let saw_side_effect =
        song_lua_side_effect_count(lua).map_err(|err| err.to_string())? > side_effect_before;
    result.map_err(|err| err.to_string())?;
    Ok((overlay_blocks, tracked_blocks, broadcasts, saw_side_effect))
}

fn compile_function_action(
    lua: &Lua,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    function: &Function,
    beat: f32,
    persists: bool,
    counter: &mut usize,
    messages: &mut Vec<SongLuaMessageEvent>,
) -> Result<bool, String> {
    let (overlay_captures, tracked_captures, broadcasts, saw_side_effect) =
        capture_function_action_blocks(lua, overlays, tracked_actors, function, beat)?;
    if !broadcasts.is_empty() && broadcasts.iter().all(|(_, has_params)| !*has_params) {
        let mut emitted = false;
        for (message, _) in broadcasts {
            if !song_lua_message_has_listener(overlays, tracked_actors, &message) {
                continue;
            }
            messages.push(SongLuaMessageEvent {
                beat,
                message,
                persists,
            });
            emitted = true;
        }
        if emitted {
            return Ok(true);
        }
    }
    if overlay_captures.is_empty() && tracked_captures.is_empty() {
        return Ok(saw_side_effect);
    }
    let message = format!("__songlua_overlay_fn_action_{}", *counter);
    *counter += 1;
    for (overlay_index, blocks) in overlay_captures {
        overlays[overlay_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                message: message.clone(),
                blocks,
            });
    }
    for (tracked_index, blocks) in tracked_captures {
        tracked_actors[tracked_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                message: message.clone(),
                blocks,
            });
    }
    messages.push(SongLuaMessageEvent {
        beat,
        message,
        persists,
    });
    Ok(true)
}

fn song_lua_message_has_listener(
    overlays: &[OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
    message: &str,
) -> bool {
    overlays.iter().any(|overlay| {
        overlay
            .actor
            .message_commands
            .iter()
            .any(|command| command.message == message)
    }) || tracked_actors.iter().any(|actor| {
        actor
            .actor
            .message_commands
            .iter()
            .any(|command| command.message == message)
    })
}

fn compile_overlay_function_ease(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    function: &Function,
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    from: f32,
    to: f32,
    easing: Option<String>,
    sustain: Option<f32>,
    opt1: Option<f32>,
    opt2: Option<f32>,
    probe_actor_ptrs: &[usize],
) -> Result<Vec<SongLuaOverlayEase>, String> {
    let start_beat = start;
    let end_beat = song_lua_span_end(start, limit, span_mode).max(start_beat);
    let overlay_indices = overlay_function_ease_indices(overlays, probe_actor_ptrs);
    let from_blocks = capture_overlay_function_blocks(
        lua,
        overlays,
        &overlay_indices,
        function,
        Some(from),
        Some(start_beat),
    )?;
    let to_blocks = capture_overlay_function_blocks(
        lua,
        overlays,
        &overlay_indices,
        function,
        Some(to),
        Some(end_beat),
    )?;
    if from_blocks.is_empty() && to_blocks.is_empty() {
        return Ok(Vec::new());
    }

    let mut from_deltas = HashMap::new();
    for (overlay_index, blocks) in from_blocks {
        if let Some(delta) = overlay_delta_from_blocks(&blocks) {
            from_deltas.insert(overlay_index, delta);
        }
    }
    let mut to_deltas = HashMap::new();
    for (overlay_index, blocks) in to_blocks {
        if let Some(delta) = overlay_delta_from_blocks(&blocks) {
            to_deltas.insert(overlay_index, delta);
        }
    }

    let mut out = Vec::new();
    for overlay_index in 0..overlays.len() {
        let Some((from_delta, to_delta)) = from_deltas
            .get(&overlay_index)
            .zip(to_deltas.get(&overlay_index))
            .and_then(|(from_delta, to_delta)| overlay_delta_intersection(from_delta, to_delta))
        else {
            continue;
        };
        out.push(SongLuaOverlayEase {
            overlay_index,
            unit,
            start,
            limit,
            span_mode,
            from: from_delta,
            to: to_delta,
            easing: easing.clone(),
            sustain,
            opt1,
            opt2,
        });
    }
    Ok(out)
}

fn overlay_function_ease_indices(
    overlays: &[OverlayCompileActor],
    probe_actor_ptrs: &[usize],
) -> Vec<usize> {
    if probe_actor_ptrs.is_empty() {
        return (0..overlays.len()).collect();
    }
    let probe_actor_ptrs: HashSet<_> = probe_actor_ptrs.iter().copied().collect();
    let out: Vec<_> = overlays
        .iter()
        .enumerate()
        .filter(|(_, overlay)| probe_actor_ptrs.contains(&(overlay.table.to_pointer() as usize)))
        .map(|(index, _)| index)
        .collect();
    if out.is_empty() {
        (0..overlays.len()).collect()
    } else {
        out
    }
}

#[inline(always)]
fn song_lua_span_end(start: f32, limit: f32, span_mode: SongLuaSpanMode) -> f32 {
    match span_mode {
        SongLuaSpanMode::Len => start + limit.max(0.0),
        SongLuaSpanMode::End => limit,
    }
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

fn set_rolling_numbers_metric(actor: &Table, metric: &str) -> mlua::Result<()> {
    actor.set("__songlua_rolling_numbers_metric", metric)?;
    actor.set(
        "__songlua_rolling_numbers_format",
        rolling_numbers_format(metric),
    )?;
    Ok(())
}

fn rolling_numbers_format(metric: &str) -> &'static str {
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

fn rolling_numbers_text(actor: &Table, number: f32) -> mlua::Result<String> {
    let format = actor
        .get::<Option<String>>("__songlua_rolling_numbers_format")?
        .unwrap_or_else(|| "%.0f".to_string());
    Ok(format_rolling_number(&format, number))
}

fn format_rolling_number(format: &str, number: f32) -> String {
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

#[cfg(test)]
mod tests;
