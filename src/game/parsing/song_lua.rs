use chrono::{Datelike, Local};
use image::image_dimensions;
use log::debug;
use mlua::{Function, Lua, MultiValue, Table, Value, ffi};
use std::collections::{HashMap, HashSet};
use std::ffi::c_int;
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::engine::present::actors::TextAlign;
use crate::engine::present::anim::EffectClock;
#[cfg(test)]
use crate::engine::present::anim::EffectMode;

#[path = "song_lua/overlay.rs"]
mod overlay;

pub use self::overlay::{
    SongLuaOverlayActor, SongLuaOverlayBlendMode, SongLuaOverlayCommandBlock, SongLuaOverlayEase,
    SongLuaOverlayKind, SongLuaOverlayMessageCommand, SongLuaOverlayState,
    SongLuaOverlayStateDelta, SongLuaProxyTarget,
};
use self::overlay::{
    overlay_delta_from_blocks, overlay_delta_intersection, overlay_state_after_blocks,
    parse_overlay_blend_mode, parse_overlay_effect_clock, parse_overlay_effect_mode,
    parse_overlay_text_align,
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
const SONG_LUA_RUNTIME_BPS_KEY: &str = "__songlua_song_bps";
const SONG_LUA_RUNTIME_RATE_KEY: &str = "__songlua_music_rate";
const SONG_LUA_SIDE_EFFECT_COUNT_KEY: &str = "__songlua_side_effect_count";
const SONG_LUA_THEME_PATH_PREFIX: &str = "__songlua_theme_path/";
const SONG_LUA_THEME_NAME: &str = "Simply Love";
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
    pub const fn default_enabled() -> Self {
        Self::Challenge
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

impl Default for SongLuaPlayerContext {
    fn default() -> Self {
        Self {
            enabled: true,
            difficulty: SongLuaDifficulty::default_enabled(),
            speedmod: SongLuaSpeedMod::default(),
            display_bpms: [60.0, 60.0],
            noteskin_name: crate::game::profile::NoteSkin::default().to_string(),
            screen_x: 320.0,
            screen_y: 240.0,
        }
    }
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaCapturedActor {
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompiledSongLua {
    pub entry_path: PathBuf,
    pub screen_width: f32,
    pub screen_height: f32,
    pub beat_mods: Vec<SongLuaModWindow>,
    pub time_mods: Vec<SongLuaModWindow>,
    pub eases: Vec<SongLuaEaseWindow>,
    pub messages: Vec<SongLuaMessageEvent>,
    pub overlays: Vec<SongLuaOverlayActor>,
    pub overlay_eases: Vec<SongLuaOverlayEase>,
    pub player_actors: [SongLuaCapturedActor; LUA_PLAYERS],
    pub song_foreground: SongLuaCapturedActor,
    pub hidden_players: [bool; LUA_PLAYERS],
    pub info: SongLuaCompileInfo,
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
    let entry_path = entry_file_path(entry_path)
        .ok_or_else(|| format!("song lua entry '{}' does not exist", entry_path.display()))?;
    let lua = Lua::new();
    let mut host = HostState::default();
    install_host(&lua, context, &mut host).map_err(|err| err.to_string())?;
    let root = execute_script_file(&lua, &entry_path, context.song_dir.as_path())
        .map_err(|err| format!("failed to execute '{}': {err}", entry_path.display()))?;
    run_actor_init_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor init commands for '{}': {err}",
            entry_path.display()
        )
    })?;
    run_actor_startup_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor startup commands for '{}': {err}",
            entry_path.display()
        )
    })?;
    run_actor_update_functions(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor update functions for '{}': {err}",
            entry_path.display()
        )
    })?;
    run_actor_draw_functions(&lua, &root);

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
    let overlays = read_overlay_actors(&lua, &root);
    restore_compile_globals(&globals, compile_globals).map_err(|err| err.to_string())?;
    let mut overlays = overlays?;
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
        out.info.unsupported_function_eases += info.unsupported_function_eases;
        let fn_actions = read_actions(
            &lua,
            prefix_globals
                .get::<Option<Table>>("actions")
                .map_err(|err| err.to_string())?,
            &mut overlays,
            &mut tracked_actors,
            &mut out.messages,
            &mut overlay_trigger_counter,
        )?;
        out.info.unsupported_function_actions += fn_actions;
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
    out.info.unsupported_function_eases += global_info.unsupported_function_eases;
    let global_fn_actions = read_actions(
        &lua,
        globals
            .get::<Option<Table>>("mod_actions")
            .map_err(|err| err.to_string())?,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
    )?;
    out.info.unsupported_function_actions += global_fn_actions;
    let update_fn_actions = read_update_function_actions(
        &lua,
        &root,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
    )?;
    out.info.unsupported_function_actions += update_fn_actions;
    let (perframe_eases, perframe_overlay_eases, unsupported_perframes) = compile_perframes(
        &lua,
        prefix_perframes,
        global_perframes,
        context,
        &mut overlays,
        &tracked_actors,
    )?;
    out.eases.extend(perframe_eases);
    out.overlay_eases.extend(perframe_overlay_eases);
    out.info.unsupported_perframes += unsupported_perframes;
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
    Ok(out)
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
    install_def(lua)?;
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
    let string: Table = globals.get("string")?;
    if matches!(string.get::<Value>("gfind")?, Value::Nil) {
        let gmatch = string.get::<Value>("gmatch")?;
        string.set("gfind", gmatch)?;
    }
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
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let path = resolve_compat_path(&listing_song_dir, raw_path.as_str());
            let entries = if !path.exists() {
                Vec::new()
            } else if path.is_dir() {
                let mut entries = fs::read_dir(&path)
                    .map_err(mlua::Error::external)?
                    .filter_map(Result::ok)
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect::<Vec<_>>();
                entries.sort_unstable();
                entries
            } else {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| vec![name.to_string()])
                    .unwrap_or_default()
            };
            let table = lua.create_table()?;
            for (idx, entry) in entries.into_iter().enumerate() {
                table.raw_set(idx + 1, entry)?;
            }
            Ok(Value::Table(table))
        })?,
    )?;
    let file_song_dir = song_dir.clone();
    fileman.set(
        "DoesFileExist",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(resolve_compat_path(&file_song_dir, raw_path.as_str()).is_file())
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
        "GetTimeSinceStart",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
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
    player_options.set("ConfusionOffset", context.confusion_offset_available)?;
    player_options.set("Confusion", context.confusion_available)?;
    player_options.set("AMod", context.amod_available)?;
    player_options.set("FromString", true)?;
    globals.set("PlayerOptions", player_options)?;

    let prefsmgr = lua.create_table()?;
    let global_offset_seconds = context.global_offset_seconds;
    let display_aspect_ratio = screen_width / screen_height.max(1.0);
    let display_width = screen_width.round() as i32;
    let display_height = screen_height.round() as i32;
    let video_renderers = "opengl".to_string();
    let bg_brightness = 1.0_f32;
    prefsmgr.set(
        "GetPreference",
        lua.create_function(move |lua, (_self, key): (Table, String)| {
            if key.eq_ignore_ascii_case("GlobalOffsetSeconds") {
                Ok(Value::Number(global_offset_seconds as f64))
            } else if key.eq_ignore_ascii_case("DisplayAspectRatio") {
                Ok(Value::Number(display_aspect_ratio as f64))
            } else if key.eq_ignore_ascii_case("DisplayWidth") {
                Ok(Value::Integer(display_width as i64))
            } else if key.eq_ignore_ascii_case("DisplayHeight") {
                Ok(Value::Integer(display_height as i64))
            } else if key.eq_ignore_ascii_case("VideoRenderers") {
                Ok(Value::String(lua.create_string(&video_renderers)?))
            } else if key.eq_ignore_ascii_case("BGBrightness") {
                Ok(Value::Number(bg_brightness as f64))
            } else if key.eq_ignore_ascii_case("TimingWindowScale") {
                Ok(Value::Number(1.0))
            } else if key.eq_ignore_ascii_case("Theme") {
                Ok(Value::String(lua.create_string(SONG_LUA_THEME_NAME)?))
            } else if key
                .to_ascii_lowercase()
                .starts_with("defaultlocalprofileid")
            {
                Ok(Value::String(lua.create_string("")?))
            } else {
                Ok(Value::Nil)
            }
        })?,
    )?;
    prefsmgr.set(
        "SetPreference",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set("PREFSMAN", prefsmgr)?;
    globals.set("DISPLAY", create_display_table(lua, context)?)?;
    globals.set("THEME", create_theme_table(lua)?)?;
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
    globals.set("SONGMAN", create_songman_table(lua, song.clone(), context)?)?;
    let players = create_player_tables(lua, context)?;
    let song_options = create_song_options_table(lua, context.song_music_rate)?;
    let gamestate = lua.create_table()?;
    let game_env = lua.create_table()?;
    gamestate.set(
        "Env",
        lua.create_function(move |_, _args: MultiValue| Ok(game_env.clone()))?,
    )?;
    let enabled_players = create_enabled_players_table(lua, context.players.clone())?;
    let human_players = enabled_players.clone();
    let song_clone = song.clone();
    gamestate.set(
        "GetCurrentSong",
        lua.create_function(move |_, _args: MultiValue| Ok(song_clone.clone()))?,
    )?;
    let style = create_style_table(lua, &context.style_name)?;
    gamestate.set(
        "GetCurrentStyle",
        lua.create_function(move |_, _args: MultiValue| Ok(style.clone()))?,
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
    let current_steps = lua.create_table()?;
    for (player_index, steps) in players.steps.iter().enumerate() {
        current_steps.raw_set(player_index + 1, steps.clone())?;
    }
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
            move |_, args: MultiValue| {
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
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    gamestate.set(
        "GetCurrentCourse",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
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
        "InsertCoin",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
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
    let top_screen = create_top_screen_table(lua, context.players.clone())?;
    globals.set(
        "__songlua_top_screen_player_1",
        top_screen.players[0].clone(),
    )?;
    globals.set(
        "__songlua_top_screen_player_2",
        top_screen.players[1].clone(),
    )?;
    globals.set("__songlua_top_screen", top_screen.top_screen.clone())?;
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
    globals.set("SOUND", create_sound_table(lua)?)?;

    let messageman = lua.create_table()?;
    messageman.set(
        "Broadcast",
        lua.create_function(|lua, args: MultiValue| {
            if let Some(message) = method_arg(&args, 0).cloned().and_then(read_string) {
                note_song_lua_side_effect(lua)?;
                broadcast_song_lua_message(lua, &message)?;
            }
            Ok(())
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
    globals.set("SL", create_sl_table(lua, context)?)?;
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
    table.set("ShowFaPlusPane", false)?;
    table.set(
        "TimingWindows",
        create_bool_array(lua, &[true, true, true, false, false])?,
    )?;
    table.set("SpeedModType", speed_type)?;
    table.set("SpeedMod", speed_value)?;
    table.set("Mini", "0%")?;
    table.set("VisualDelay", "0ms")?;
    table.set("BackgroundFilter", 0)?;
    table.set("HideLookahead", false)?;
    table.set("NoteFieldOffsetX", 0)?;
    table.set("NoteFieldOffsetY", 0)?;
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
    table.set("PeakNPS", 0.0)?;
    table.set("Hash", "")?;
    Ok(table)
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

fn create_screen_timer_actor(lua: &Lua) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "Timer")?;
    actor.set(
        "GetSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(actor)
}

fn create_note_field_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteField")?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_player_child_name", "NoteField")?;
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
    players: [SongLuaPlayerContext; LUA_PLAYERS],
) -> mlua::Result<TopScreenLuaTables> {
    let top_screen = create_dummy_actor(lua, "TopScreen")?;
    top_screen.set("Name", "ScreenGameplay")?;
    let top_screen_for_get_child = top_screen.clone();
    let player_actors = [
        create_top_screen_player_actor(lua, players[0].clone(), 0)?,
        create_top_screen_player_actor(lua, players[1].clone(), 1)?,
    ];
    for player_actor in &player_actors {
        player_actor.set("__songlua_parent", top_screen.clone())?;
    }
    let player_actors_for_get_child = player_actors.clone();
    top_screen.set(
        "GetChild",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            if let Some(player_index) = top_screen_player_index(&name) {
                return if players[player_index].enabled {
                    Ok(Value::Table(
                        player_actors_for_get_child[player_index].clone(),
                    ))
                } else {
                    Ok(Value::Nil)
                };
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
        "GetCurrentRowIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
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
    let music_wheel = create_music_wheel_table(lua)?;
    top_screen.set(
        "GetMusicWheel",
        lua.create_function(move |_, _args: MultiValue| Ok(music_wheel.clone()))?,
    )?;
    top_screen.set(
        "GetNextCourseSong",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    for name in [
        "AddInputCallback",
        "RemoveInputCallback",
        "PostScreenMessage",
        "SetProfileIndex",
        "Cancel",
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

fn create_music_wheel_table(lua: &Lua) -> mlua::Result<Table> {
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
    for name in ["ChangeSort", "SetOpenSection"] {
        wheel.set(
            name,
            lua.create_function({
                let wheel = wheel.clone();
                move |lua, _args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    Ok(wheel.clone())
                }
            })?,
        )?;
    }
    Ok(wheel)
}

fn create_top_screen_player_actor(
    lua: &Lua,
    player: SongLuaPlayerContext,
    player_index: usize,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "PlayerActor")?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_visible", true)?;
    actor.set("__songlua_state_x", player.screen_x)?;
    actor.set("__songlua_state_y", player.screen_y)?;
    Ok(actor)
}

fn top_screen_player_index(name: &str) -> Option<usize> {
    match name {
        "PlayerP1" => Some(0),
        "PlayerP2" => Some(1),
        _ => None,
    }
}

fn create_player_tables(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<PlayerLuaTables> {
    let player_states = [
        create_player_state_table(lua, context.players[0].clone())?,
        create_player_state_table(lua, context.players[1].clone())?,
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

fn create_theme_table(lua: &Lua) -> mlua::Result<Table> {
    let theme = lua.create_table()?;
    set_string_method(lua, &theme, "GetCurThemeName", SONG_LUA_THEME_NAME)?;
    set_string_method(lua, &theme, "GetThemeDisplayName", SONG_LUA_THEME_NAME)?;
    set_string_method(
        lua,
        &theme,
        "GetCurrentThemeDirectory",
        SONG_LUA_THEME_PATH_PREFIX,
    )?;
    theme.set(
        "GetMetric",
        lua.create_function(|lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            theme_metric_value(lua, &group, &name)
        })?,
    )?;
    theme.set(
        "GetMetricF",
        lua.create_function(|_, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            Ok(theme_metric_number(&group, &name)
                .map_or(Value::Nil, |value| Value::Number(value as f64)))
        })?,
    )?;
    theme.set(
        "GetMetricI",
        lua.create_function(|_, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            Ok(theme_metric_number(&group, &name)
                .map_or(Value::Nil, |value| Value::Integer(value.round() as i64)))
        })?,
    )?;
    theme.set(
        "GetMetricB",
        lua.create_function(|lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(theme_metric_bool(theme_metric_value(lua, &group, &name)?))
        })?,
    )?;
    theme.set(
        "HasMetric",
        lua.create_function(|lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(!matches!(
                theme_metric_value(lua, &group, &name)?,
                Value::Nil
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
    Ok(theme)
}

fn create_sound_table(lua: &Lua) -> mlua::Result<Table> {
    let sound = lua.create_table()?;
    for name in ["DimMusic", "PlayMusicPart", "PlayOnce"] {
        sound.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(sound)
}

fn theme_metric_value(lua: &Lua, group: &str, name: &str) -> mlua::Result<Value> {
    if let Some(value) = theme_metric_number(group, name) {
        return Ok(Value::Number(value as f64));
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

fn theme_metric_number(group: &str, name: &str) -> Option<f32> {
    if group.eq_ignore_ascii_case("Player") {
        if name.eq_ignore_ascii_case("ReceptorArrowsYStandard") {
            return Some(THEME_RECEPTOR_Y_STD);
        }
        if name.eq_ignore_ascii_case("ReceptorArrowsYReverse") {
            return Some(THEME_RECEPTOR_Y_REV);
        }
    }
    if group.eq_ignore_ascii_case("Combo") && name.eq_ignore_ascii_case("ShowComboAt") {
        return Some(4.0);
    }
    if group.eq_ignore_ascii_case("GraphDisplay") {
        if name.eq_ignore_ascii_case("BodyWidth") {
            return Some(300.0);
        }
        if name.eq_ignore_ascii_case("BodyHeight") {
            return Some(120.0);
        }
    }
    if group.eq_ignore_ascii_case("LifeMeterBar") && name.eq_ignore_ascii_case("InitialValue") {
        return Some(0.5);
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

fn theme_metric_bool(value: Value) -> bool {
    match value {
        Value::Boolean(value) => value,
        Value::Integer(value) => value != 0,
        Value::Number(value) => value != 0.0,
        Value::String(value) => !value.to_str().is_ok_and(|text| text.is_empty()),
        _ => false,
    }
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

fn create_theme_prefs_table(lua: &Lua) -> mlua::Result<Table> {
    let prefs = lua.create_table()?;
    prefs.set(
        "Get",
        lua.create_function(|lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            theme_pref_default(lua, &name)
        })?,
    )?;
    prefs.set(
        "Set",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
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
    Ok(prefs)
}

fn create_profileman_table(lua: &Lua) -> mlua::Result<Table> {
    let profileman = lua.create_table()?;
    profileman.set("__songlua_stats_prefix", "")?;
    let machine_profile = create_profile_table(lua, "Machine")?;
    profileman.set(
        "GetMachineProfile",
        lua.create_function(move |_, _args: MultiValue| Ok(machine_profile.clone()))?,
    )?;
    profileman.set(
        "GetProfile",
        lua.create_function(|lua, args: MultiValue| {
            let player_name = method_arg(&args, 0)
                .and_then(player_index_from_value)
                .map(|index| format!("Player {}", index + 1))
                .unwrap_or_else(|| "Player".to_string());
            create_profile_table(lua, &player_name)
        })?,
    )?;
    profileman.set(
        "GetProfileDir",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
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
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    profileman.set(
        "GetLocalProfileIDFromIndex",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    profileman.set(
        "GetLocalProfileFromIndex",
        lua.create_function(|lua, _args: MultiValue| create_profile_table(lua, "Local Profile"))?,
    )?;
    profileman.set(
        "SaveMachineProfile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
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
                Ok(())
            }
        })?,
    )?;
    Ok(profileman)
}

fn create_profile_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let profile = lua.create_table()?;
    profile.set("__songlua_last_score_name", name)?;
    set_string_method(lua, &profile, "GetDisplayName", name)?;
    set_string_method(lua, &profile, "GetGUID", "")?;
    set_string_method(lua, &profile, "GetProfileDir", "")?;
    profile.set(
        "GetLastUsedHighScoreName",
        lua.create_function({
            let profile = profile.clone();
            move |lua, _args: MultiValue| {
                let name = profile
                    .get::<Option<String>>("__songlua_last_score_name")?
                    .unwrap_or_default();
                Ok(Value::String(lua.create_string(&name)?))
            }
        })?,
    )?;
    profile.set(
        "SetLastUsedHighScoreName",
        lua.create_function({
            let profile = profile.clone();
            move |lua, args: MultiValue| {
                let name = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                profile.set("__songlua_last_score_name", name)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    for name in [
        "GetCaloriesBurnedToday",
        "GetNumTotalSongsPlayed",
        "GetTotalSessions",
        "CalculateCaloriesFromHeartRate",
    ] {
        profile.set(name, lua.create_function(|_, _args: MultiValue| Ok(0_i64))?)?;
    }
    profile.set(
        "GetIgnoreStepCountCalories",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    profile.set(
        "AddCaloriesToDailyTotal",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
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
    Ok(profile)
}

fn create_statsman_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let statsman = lua.create_table()?;
    let stage_stats = create_stage_stats_table(lua, context)?;
    statsman.set(
        "GetCurStageStats",
        lua.create_function({
            let stage_stats = stage_stats.clone();
            move |_, _args: MultiValue| Ok(stage_stats.clone())
        })?,
    )?;
    statsman.set(
        "GetPlayedStageStats",
        lua.create_function(move |_, _args: MultiValue| Ok(stage_stats.clone()))?,
    )?;
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
        "AllFailed",
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
    let high_score = create_high_score_table(lua, high_score_name)?;
    let played_steps = lua.create_table()?;
    played_steps.raw_set(
        1,
        create_steps_table(lua, player.difficulty, player.display_bpms, song_dir)?,
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
                Ok(())
            }
        })?,
    )?;
    stats.set(
        "GetScore",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "SetScore",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    stats.set(
        "GetPercentDancePoints",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetActualDancePoints",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetPossibleDancePoints",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    stats.set(
        "GetCurrentPossibleDancePoints",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    stats.set(
        "GetCurrentLife",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
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
        "GetLifeRecord",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    stats.set(
        "GetTapNoteScores",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetPlayedSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(played_steps.clone()))?,
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
    stats.set(
        "IsDisqualified",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stats.set(
        "FullComboOfScore",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(stats)
}

fn create_stage_radar_values_table(lua: &Lua, fallback: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetValue",
        lua.create_function(move |_, _args: MultiValue| Ok(fallback))?,
    )?;
    Ok(table)
}

fn create_high_score_list_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let high_scores = lua.create_table()?;
    high_scores.raw_set(1, create_high_score_table(lua, "Machine")?)?;
    table.set(
        "GetHighScores",
        lua.create_function(move |_, _args: MultiValue| Ok(high_scores.clone()))?,
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
    set_string_method(lua, &score, "GetStageAward", "StageAward_None")?;
    Ok(score)
}

fn create_display_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let display = lua.create_table()?;
    let width = context.screen_width.max(1.0).round() as i32;
    let height = context.screen_height.max(1.0).round() as i32;
    let specs = lua.create_table()?;

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

fn create_player_state_table(lua: &Lua, player: SongLuaPlayerContext) -> mlua::Result<Table> {
    let options = create_player_options_table(lua, player)?;
    let table = lua.create_table()?;
    table.set("__songlua_player_options_string", String::new())?;
    table.set(
        "GetPlayerOptions",
        lua.create_function(move |_, _args: MultiValue| Ok(options.clone()))?,
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
        "SetPlayerOptions",
        lua.create_function(|_, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(());
            };
            let options = method_arg(&args, 1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            owner.set("__songlua_player_options_string", options)?;
            Ok(())
        })?,
    )?;
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
    install_speedmod_method(lua, &table, "MMod", player.speedmod, SongLuaSpeedMod::M)?;
    install_speedmod_method(lua, &table, "AMod", player.speedmod, SongLuaSpeedMod::A)?;
    install_speedmod_method(lua, &table, "XMod", player.speedmod, SongLuaSpeedMod::X)?;
    for name in ["Mirror", "Left", "Right", "Reverse", "Mini", "Skew", "Tilt"] {
        table.set(name, create_player_option_method(lua, &table, name)?)?;
    }
    table.set(
        "GetReversePercentForColumn",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
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
                owner.set("__songlua_noteskin_name", noteskin_name)?;
                return Ok(Value::Table(owner));
            }
            let noteskin_name = owner
                .get::<Option<String>>("__songlua_noteskin_name")?
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
            if name.eq_ignore_ascii_case("FromString") {
                return Ok(Value::Function(
                    lua.create_function(|_, _args: MultiValue| Ok(()))?,
                ));
            }
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

fn create_player_option_method(lua: &Lua, owner: &Table, name: &str) -> mlua::Result<Function> {
    let owner = owner.clone();
    let name = name.to_ascii_lowercase();
    lua.create_function(move |lua, args: MultiValue| {
        let state = player_option_state(lua, &owner)?;
        if let Some(value) = method_arg(&args, 0).cloned() {
            state.set(name.as_str(), normalize_player_option_value(&name, value))?;
            return Ok(Value::Table(owner.clone()));
        }
        Ok(state
            .get::<Option<Value>>(name.as_str())?
            .unwrap_or_else(|| default_player_option_value(&name)))
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

#[inline(always)]
fn normalize_player_option_value(name: &str, value: Value) -> Value {
    if player_option_uses_bool(name) {
        return Value::Boolean(read_boolish(value).unwrap_or(false));
    }
    Value::Number(read_f32(value).unwrap_or(0.0) as f64)
}

#[inline(always)]
fn default_player_option_value(name: &str) -> Value {
    if player_option_uses_bool(name) {
        Value::Boolean(false)
    } else {
        Value::Number(0.0)
    }
}

#[inline(always)]
fn player_option_uses_bool(name: &str) -> bool {
    matches!(
        name,
        "mirror" | "left" | "right" | "stealthtype" | "stealthpastreceptors"
    )
}

fn install_speedmod_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    speedmod: SongLuaSpeedMod,
    ctor: fn(f32) -> SongLuaSpeedMod,
) -> mlua::Result<()> {
    table.set(
        name,
        lua.create_function(move |_, _args: MultiValue| Ok(speedmod_value(speedmod, ctor)))?,
    )
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
        "GetStepsByStepsType",
        lua.create_function(move |_, _args: MultiValue| Ok(steps_by_type.clone()))?,
    )?;
    Ok(table)
}

fn create_songman_table(
    lua: &Lua,
    current_song: Table,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
    let songman = lua.create_table()?;
    songman.set("__songlua_preferred_sort_songs", false)?;
    let current_group = song_group_name(&context.song_dir);
    let current_title = context.main_title.clone();
    let current_dir = song_dir_string(context.song_dir.as_path());
    let all_songs = create_single_value_array(lua, current_song.clone())?;
    let groups = create_string_array(lua, &[current_group.as_str()])?;
    let group_songs = create_single_value_array(lua, current_song.clone())?;
    let group = create_song_group_table(lua)?;

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
        "GetAllSongs",
        lua.create_function(move |_, _args: MultiValue| Ok(all_songs.clone()))?,
    )?;
    songman.set(
        "GetSongGroupNames",
        lua.create_function(move |_, _args: MultiValue| Ok(groups.clone()))?,
    )?;
    songman.set(
        "GetSongsInGroup",
        lua.create_function({
            let current_group = current_group.clone();
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
        "DoesSongGroupExist",
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
    songman.set(
        "GetGroup",
        lua.create_function(move |_, _args: MultiValue| Ok(group.clone()))?,
    )?;
    songman.set(
        "GetNumSongGroups",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    songman.set(
        "GetNumSongs",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    songman.set(
        "GetAllCourses",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    songman.set(
        "SetPreferredSongs",
        lua.create_function({
            let songman = songman.clone();
            move |lua, _args: MultiValue| {
                songman.set("__songlua_preferred_sort_songs", true)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    songman.set(
        "GetPreferredSortSongs",
        lua.create_function({
            let songman = songman.clone();
            move |_, _args: MultiValue| {
                Ok(songman
                    .get::<Option<bool>>("__songlua_preferred_sort_songs")?
                    .unwrap_or(false))
            }
        })?,
    )?;
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

fn create_song_runtime_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(SONG_LUA_RUNTIME_BEAT_KEY, 0.0_f32)?;
    table.set(SONG_LUA_RUNTIME_SECONDS_KEY, 0.0_f32)?;
    table.set(SONG_LUA_RUNTIME_BPS_KEY, song_display_bps(context))?;
    table.set(SONG_LUA_RUNTIME_RATE_KEY, song_music_rate(context))?;
    Ok(table)
}

fn create_song_position_table(lua: &Lua, song_runtime: &Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetSongBeat",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| Ok(song_runtime.get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)?)
        })?,
    )?;
    Ok(table)
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
    table.set(
        "GetName",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&style_name)?))
        })?,
    )?;
    Ok(table)
}

fn install_def(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let def = lua.create_table()?;
    for &(name, actor_type) in &[
        ("Actor", "Actor"),
        ("ActorFrame", "ActorFrame"),
        ("Sprite", "Sprite"),
        ("Sound", "Sound"),
        ("BitmapText", "BitmapText"),
        ("Model", "Model"),
        ("Quad", "Quad"),
        ("ActorProxy", "ActorProxy"),
        ("ActorFrameTexture", "ActorFrameTexture"),
    ] {
        def.set(name, make_actor_ctor(lua, actor_type)?)?;
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
        "GetState",
        "SetStateProperties",
        "GetAnimationLengthSeconds",
        "SetSecondsIntoAnimation",
        "SetEffectMode",
    ] {
        class.set(
            method_name,
            lua.create_function({
                let method_name = method_name.to_string();
                move |_, args: MultiValue| {
                    let Some(Value::Table(actor)) = args.front() else {
                        return Ok(Value::Nil);
                    };
                    match actor.get::<Value>(method_name.as_str())? {
                        Value::Function(method) => method.call::<Value>(args),
                        _ => Ok(Value::Nil),
                    }
                }
            })?,
        )?;
    }
    Ok(class)
}

fn create_actorframe_class_table(lua: &Lua) -> mlua::Result<Table> {
    let class = lua.create_table()?;
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
    class.set(
        "GetChildAt",
        lua.create_function(|_, args: MultiValue| {
            let Some(actor) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(Value::Nil);
            };
            let Some(index) = args.get(1).and_then(|value| match value {
                Value::Integer(value) if *value >= 0 => Some(*value as usize),
                Value::Number(value) if value.is_finite() && *value >= 0.0 => Some(*value as usize),
                _ => None,
            }) else {
                return Ok(Value::Nil);
            };
            let lua_index = if index == 0 { 1 } else { index };
            Ok(match actor.raw_get::<Option<Value>>(lua_index)? {
                Some(Value::Table(child)) => Value::Table(child),
                _ => Value::Nil,
            })
        })?,
    )?;
    Ok(class)
}

fn make_actor_ctor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Function> {
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
    run_actor_update_functions_for_table(lua, root)
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
) -> Result<usize, String> {
    let Value::Table(root) = root else {
        return Ok(0);
    };
    let globals = lua.globals();
    let Some(debug) = globals
        .get::<Option<Table>>("debug")
        .map_err(|err| err.to_string())?
    else {
        return Ok(0);
    };
    let Some(getupvalue) = debug
        .get::<Option<Function>>("getupvalue")
        .map_err(|err| err.to_string())?
    else {
        return Ok(0);
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
    )
}

fn run_actor_init_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    run_actor_init_command(lua, actor)?;
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_init_commands_for_table(lua, &child)?;
    }
    Ok(())
}

fn run_actor_startup_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
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
    drain_actor_command_queue(lua, actor)
}

fn run_actor_update_functions_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(update) = actor.get::<Option<Function>>("__songlua_update_function")? {
        call_actor_function(lua, actor, &update)?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_update_functions_for_table(lua, &child)?;
    }
    Ok(())
}

fn run_actor_draw_functions_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(draw) = actor.get::<Option<Function>>("__songlua_draw_function")? {
        let draw_result = call_actor_function(lua, actor, &draw);
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
) -> Result<usize, String> {
    let mut unsupported = 0usize;
    if let Some(update) = actor
        .get::<Option<Function>>("__songlua_update_function")
        .map_err(|err| err.to_string())?
    {
        for table in update_function_action_tables(getupvalue, &update, seen_tables)? {
            unsupported += read_actions(
                lua,
                Some(table),
                overlays,
                tracked_actors,
                messages,
                counter,
            )?;
        }
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        unsupported += read_update_function_actions_for_table(
            lua,
            &child,
            getupvalue,
            overlays,
            tracked_actors,
            messages,
            counter,
            seen_tables,
        )?;
    }
    Ok(unsupported)
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
    let Some(command) = actor.get::<Option<Function>>(name)? else {
        return Ok(());
    };
    run_guarded_actor_command(lua, actor, name, &command, drain_queue)
}

fn actor_runs_startup_commands(actor: &Table) -> mlua::Result<bool> {
    let _ = actor;
    Ok(true)
}

fn call_actor_function(lua: &Lua, actor: &Table, command: &Function) -> mlua::Result<()> {
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        return call_with_script_dir(lua, Path::new(&script_dir), || {
            command.call::<()>(actor.clone())
        });
    }
    command.call::<()>(actor.clone())
}

fn run_guarded_actor_command(
    lua: &Lua,
    actor: &Table,
    name: &str,
    command: &Function,
    drain_queue: bool,
) -> mlua::Result<()> {
    let active = actor_active_commands(lua, actor)?;
    if active.get::<Option<bool>>(name)?.unwrap_or(false) {
        return Ok(());
    }
    active.set(name, true)?;
    let result = call_actor_function(lua, actor, command).map_err(|err| {
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

fn read_overlay_actors(lua: &Lua, root: &Value) -> Result<Vec<OverlayCompileActor>, String> {
    let Value::Table(root) = root else {
        return Ok(Vec::new());
    };
    let mut aft_capture_names = HashSet::new();
    collect_aft_capture_names(root, &mut aft_capture_names)?;
    let mut out = Vec::new();
    read_overlay_actors_from_table(lua, root, None, &aft_capture_names, &mut out)?;
    Ok(out)
}

fn read_overlay_actors_from_table(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    aft_capture_names: &HashSet<String>,
    out: &mut Vec<OverlayCompileActor>,
) -> Result<(), String> {
    let next_parent_index =
        if let Some(overlay) = read_overlay_actor(lua, actor, parent_index, aft_capture_names)? {
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
        read_overlay_actors_from_table(lua, &child, next_parent_index, aft_capture_names, out)?;
    }
    Ok(())
}

fn read_overlay_actor(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    aft_capture_names: &HashSet<String>,
) -> Result<Option<OverlayCompileActor>, String> {
    let Some(actor_type) = actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let on_command = capture_actor_command(lua, actor, "OnCommand")?;
    let initial_state =
        overlay_state_after_blocks(actor_overlay_initial_state(actor)?, &on_command, 0.0);
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
        let blocks = match capture_actor_command(lua, actor, name.as_str()) {
            Ok(blocks) => blocks,
            Err(err) => {
                debug!(
                    "Skipping song lua overlay message capture for {}.{}: {}",
                    actor_debug_label(actor),
                    name,
                    err
                );
                continue;
            }
        };
        if !blocks.is_empty() {
            message_commands.push(SongLuaOverlayMessageCommand { message, blocks });
        }
    }
    let name = actor
        .get::<Option<String>>("Name")
        .map_err(|err| err.to_string())?;

    let kind = if actor_type.eq_ignore_ascii_case("ActorFrame") {
        if parent_index.is_none()
            && name.is_none()
            && initial_state == SongLuaOverlayState::default()
            && message_commands.is_empty()
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
        if let Some(capture_name) = actor_aft_capture_name(actor).map_err(|err| err.to_string())? {
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
    } else if actor_type.eq_ignore_ascii_case("BitmapText") {
        let Some(font_path) = actor
            .get::<Option<String>>("Font")
            .map_err(|err| err.to_string())?
            .and_then(|font| resolve_actor_asset_path(actor, &font).ok())
        else {
            return Ok(None);
        };
        SongLuaOverlayKind::BitmapText {
            font_name: song_lua_font_name(font_path.as_path()),
            font_path,
            text: Arc::<str>::from(
                actor
                    .get::<Option<String>>("Text")
                    .map_err(|err| err.to_string())?
                    .unwrap_or_default(),
            ),
            stroke_color: read_actor_color_field(actor, "__songlua_stroke_color")?
                .or_else(|| read_actor_color_field(actor, "StrokeColor").ok().flatten()),
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
        .get::<Option<i32>>("__songlua_state_draw_order")
        .map_err(|err| err.to_string())?
    {
        state.draw_order = value;
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
    call_actor_function(lua, actor, &command).map_err(|err| err.to_string())?;
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    read_actor_capture_blocks(actor)
}

fn reset_actor_capture(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set("__songlua_capture_cursor", 0.0_f32)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    actor.set("__songlua_capture_blocks", lua.create_table()?)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

fn actor_current_capture_block(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
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
                draw_order: block
                    .get::<Option<i32>>("draw_order")
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

fn actor_is_bitmap_text(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("BitmapText")))
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

fn broadcast_song_lua_message(lua: &Lua, message: &str) -> mlua::Result<()> {
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
    for actor in actors {
        run_actor_named_command_with_drain(lua, &actor, &command, true)?;
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
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    actor.set(
        "sleep",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
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
                let Some(name) = args.get(1).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let command_name = format!("{name}Command");
                run_actor_named_command(lua, &actor, &command_name)?;
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
                let command = format!("{name}Command");
                for child in actor_direct_children(lua, &actor)? {
                    run_actor_named_command(lua, &child, &command)?;
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
                run_named_command_on_leaves(lua, &actor, &format!("{name}Command"))?;
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
                move |_, args: MultiValue| {
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
                    actor.set("__songlua_update_rate", value)?;
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
                if let Some(Value::Function(function)) = args.get(1).cloned() {
                    actor.set("__songlua_update_function", function)?;
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
                let delay = actor
                    .get::<Option<f32>>("__songlua_state_sprite_state_delay")?
                    .unwrap_or(0.1)
                    .max(0.0);
                let frame_count = actor_sprite_frame_count(&actor)?.max(1);
                let state = if delay <= f32::EPSILON {
                    0
                } else {
                    ((seconds.max(0.0) / delay).floor() as u32) % frame_count
                };
                set_actor_sprite_state(lua, &actor, state)?;
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
                capture_block_set_bool(lua, &actor, "sprite_animate", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "pause",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "sprite_animate", false)?;
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
                set_actor_effect_mode(lua, &actor, "none")?;
                capture_block_set_vec3(lua, &actor, "effect_magnitude", [0.0, 0.0, 0.0])?;
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
        "AddChildFromPath",
        "Create",
        "SetTextureFiltering",
        "SetAmbientLightColor",
        "SetDiffuseLightColor",
        "SetDrawByZPosition",
        "SetLightDirection",
        "SetSpecularLightColor",
        "SortByDrawOrder",
        "distort",
        "fardistz",
        "hibernate",
        "hurrytweening",
        "jitter",
        "load",
        "LoadFromSongBackground",
        "AddAttribute",
        "backfacecull",
        "ClearAttributes",
        "diffusebottomedge",
        "diffuseleftedge",
        "diffuselowerleft",
        "diffuselowerright",
        "diffuserightedge",
        "diffusetopedge",
        "diffuseupperleft",
        "diffuseupperright",
        "rainbow",
        "rainbowscroll",
        "StartTransitioningScreen",
        "propagate",
        "stop",
        "undistort",
        "volume",
        "cullmode",
        "zbias",
        "zbuffer",
        "ztest",
        "ztestmode",
        "zwrite",
    ] {
        actor.set(name, make_actor_chain_method(lua, actor)?)?;
    }
    actor.set(
        "finishtweening",
        make_actor_stop_tweening_method(lua, actor)?,
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
                move |_, args: MultiValue| {
                    let value = method_arg(&args, 0)
                        .cloned()
                        .and_then(read_boolish)
                        .unwrap_or(true);
                    actor.set("__songlua_mult_attrs_with_diffuse", value)?;
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
                    .get::<Option<bool>>("__songlua_mult_attrs_with_diffuse")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "textglowmode",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(mode) = method_arg(&args, 0).cloned().and_then(read_string) {
                    actor.set("__songlua_text_glow_mode", mode)?;
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
            move |_, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                actor.set("__songlua_max_dimension_uses_zoom", value)?;
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
        "RunCommandsOnChildren",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let Some(command) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Function(function) => Some(function),
                    _ => None,
                }) else {
                    return Ok(actor.clone());
                };
                for index in 1..=actor.raw_len() {
                    let Some(Value::Table(child)) = actor.raw_get::<Option<Value>>(index)? else {
                        continue;
                    };
                    let _ = command.call::<Value>(child)?;
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
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
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

fn make_actor_chain_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |_, _args: MultiValue| Ok(actor.clone()))
}

fn make_actor_stop_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |_, _args: MultiValue| {
        flush_actor_capture(&actor)?;
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

fn run_named_command_on_leaves(lua: &Lua, actor: &Table, command: &str) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_named_command_on_leaves(lua, &child, command)?;
    }
    if !saw_child {
        run_actor_named_command(lua, actor, command)?;
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
    lua.create_function(move |_, args: MultiValue| {
        flush_actor_capture(&actor)?;
        actor.set(
            "__songlua_capture_duration",
            args.get(1)
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0)
                .max(0.0),
        )?;
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
                let (probed_target, probe_methods) =
                    probe_function_ease_target(lua, &function).map_err(|err| err.to_string())?;
                let target = probed_target.unwrap_or(SongLuaEaseTarget::Function);
                if matches!(target, SongLuaEaseTarget::Function) {
                    match compile_overlay_function_ease(
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
                    ) {
                        Ok(compiled) if !compiled.is_empty() => {
                            overlay_eases.extend(compiled);
                            continue;
                        }
                        _ => {
                            info.unsupported_function_eases += 1;
                            debug!(
                                "Unsupported song lua function ease: unit={:?} start={start:.3} limit={limit:.3} span={:?} from={from:.3} to={to:.3} easing={:?} probe_methods={:?}",
                                unit, span_mode, easing, probe_methods
                            );
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
    Ok((out, overlay_eases, info))
}

fn record_probe_method_call(lua: &Lua, actor: &Table, method_name: &str) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(calls) = globals.get::<Option<Table>>("__songlua_probe_methods")? else {
        return Ok(());
    };
    calls.raw_set(
        calls.raw_len() + 1,
        format!("{}.{}", probe_target_kind(actor)?, method_name),
    )?;
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
) -> mlua::Result<(Option<SongLuaEaseTarget>, Vec<String>)> {
    let globals = lua.globals();
    let previous_time = compile_song_runtime_values(lua)?;
    set_compile_song_runtime_beat(lua, 0.0)?;
    let previous = globals.get::<Value>("__songlua_probe_methods")?;
    let calls = lua.create_table()?;
    globals.set("__songlua_probe_methods", calls.clone())?;
    let result = function.call::<Value>(1.0_f32);
    let methods = probe_call_names(&calls)?;
    let classify = classify_function_ease_probe(&calls);
    globals.set("__songlua_probe_methods", previous)?;
    set_compile_song_runtime_values(lua, previous_time.0, previous_time.1)?;
    match result {
        Ok(_) => Ok((classify?, methods)),
        Err(_) => Ok((None, methods)),
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
        if !matches!(target_kind, "player" | "notefield") {
            return Ok(None);
        }
        match method_name {
            "x" if target_kind == "player" => saw_x = true,
            "y" if target_kind == "player" => saw_y = true,
            "z" => saw_z = true,
            "rotationx" => saw_rotation_x = true,
            "rotationz" => saw_rotation_z = true,
            "rotationy" => saw_rotation_y = true,
            "skewx" => saw_skew_x = true,
            "skewy" => saw_skew_y = true,
            "zoom" => saw_zoom = true,
            "zoomx" => saw_zoom_x = true,
            "zoomy" => saw_zoom_y = true,
            "zoomz" => saw_zoom_z = true,
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
    for overlay in overlays {
        reset_actor_capture(lua, &overlay.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn reset_tracked_capture_tables(
    lua: &Lua,
    tracked_actors: &[TrackedCompileActor],
) -> Result<(), String> {
    for actor in tracked_actors {
        reset_actor_capture(lua, &actor.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn collect_overlay_capture_blocks(
    overlays: &[OverlayCompileActor],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for (idx, overlay) in overlays.iter().enumerate() {
        flush_actor_capture(&overlay.table).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(&overlay.table)?;
        if !blocks.is_empty() {
            out.push((idx, blocks));
        }
    }
    Ok(out)
}

fn collect_tracked_capture_blocks(
    tracked_actors: &[TrackedCompileActor],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for (idx, actor) in tracked_actors.iter().enumerate() {
        flush_actor_capture(&actor.table).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(&actor.table)?;
        if !blocks.is_empty() {
            out.push((idx, blocks));
        }
    }
    Ok(out)
}

fn capture_overlay_function_blocks(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    function: &Function,
    arg: Option<f32>,
    song_beat: Option<f32>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    if let Some(song_beat) = song_beat {
        set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    }
    reset_overlay_capture_tables(lua, overlays)?;
    let result = match arg {
        Some(value) => function.call::<Value>(value),
        None => function.call::<Value>(()),
    };
    let blocks = collect_overlay_capture_blocks(overlays);
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
        bool,
    ),
    String,
> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    reset_overlay_capture_tables(lua, overlays)?;
    reset_tracked_capture_tables(lua, tracked_actors)?;
    let result = function.call::<Value>(());
    let overlay_blocks = collect_overlay_capture_blocks(overlays);
    let tracked_blocks = collect_tracked_capture_blocks(tracked_actors);
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let overlay_blocks = overlay_blocks?;
    let tracked_blocks = tracked_blocks?;
    let saw_side_effect =
        song_lua_side_effect_count(lua).map_err(|err| err.to_string())? > side_effect_before;
    result.map_err(|err| err.to_string())?;
    Ok((overlay_blocks, tracked_blocks, saw_side_effect))
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
    let (overlay_captures, tracked_captures, saw_side_effect) =
        capture_function_action_blocks(lua, overlays, tracked_actors, function, beat)?;
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
) -> Result<Vec<SongLuaOverlayEase>, String> {
    let start_beat = start;
    let end_beat = song_lua_span_end(start, limit, span_mode).max(start_beat);
    let from_blocks =
        capture_overlay_function_blocks(lua, overlays, function, Some(from), Some(start_beat))?;
    let to_blocks =
        capture_overlay_function_blocks(lua, overlays, function, Some(to), Some(end_beat))?;
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
    delta_seconds: f32,
) -> Result<bool, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    let result = entry
        .function
        .call::<Value>((beat, delta_seconds))
        .map(|_| ())
        .map_err(|err| err.to_string());
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
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
) -> Result<(Vec<SongLuaEaseWindow>, Vec<SongLuaOverlayEase>, usize), String> {
    let mut entries = read_perframe_entries(prefix_table)?;
    entries.extend(read_perframe_entries(global_table)?);
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new(), 0));
    }

    let mut boundaries = entries
        .iter()
        .flat_map(|entry| [entry.start, entry.end])
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    boundaries.sort_by(|left, right| left.total_cmp(right));
    boundaries.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);
    if boundaries.len() < 2 {
        return Ok((Vec::new(), Vec::new(), 0));
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
            let delta_seconds = prev_eval
                .map(|prev| perframe_delta_seconds(context, (eval_beat - prev).abs()))
                .unwrap_or(0.0);
            reset_overlay_capture_tables(lua, overlays)?;
            reset_tracked_capture_tables(lua, tracked_actors)?;
            for entry in &active {
                saw_recognized_side_effect |=
                    call_perframe_entry(lua, entry, eval_beat, delta_seconds)?;
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

    let unsupported = usize::from(
        out_eases.is_empty() && out_overlay_eases.is_empty() && !saw_recognized_side_effect,
    ) * entries.len();
    Ok((out_eases, out_overlay_eases, unsupported))
}

fn read_actions(
    lua: &Lua,
    table: Option<Table>,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    counter: &mut usize,
) -> Result<usize, String> {
    let Some(table) = table else {
        return Ok(0);
    };
    let mut unsupported_function_actions = 0usize;
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
                    unsupported_function_actions += 1;
                    debug!(
                        "Unsupported song lua function action: beat={beat:.3} persists={persists}"
                    );
                }
            }
            _ => {}
        }
    }
    Ok(unsupported_function_actions)
}

#[inline(always)]
fn read_f32(value: Value) -> Option<f32> {
    match value {
        Value::Integer(value) => Some(value as f32),
        Value::Number(value) => {
            let value = value as f32;
            value.is_finite().then_some(value)
        }
        Value::String(text) => text.to_str().ok()?.trim().parse::<f32>().ok(),
        _ => None,
    }
}

#[inline(always)]
fn read_boolish(value: Value) -> Option<bool> {
    match value {
        Value::Boolean(value) => Some(value),
        Value::Integer(value) => Some(value != 0),
        Value::Number(value) => Some(value != 0.0),
        Value::String(text) => {
            let text = text.to_str().ok()?.trim().to_string();
            if text.eq_ignore_ascii_case("true")
                || text.eq_ignore_ascii_case("yes")
                || text.eq_ignore_ascii_case("on")
            {
                Some(true)
            } else if text.eq_ignore_ascii_case("false")
                || text.eq_ignore_ascii_case("no")
                || text.eq_ignore_ascii_case("off")
            {
                Some(false)
            } else {
                text.parse::<f32>().ok().map(|value| value != 0.0)
            }
        }
        _ => None,
    }
}

#[inline(always)]
fn read_string(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_str().ok()?.to_string()),
        _ => None,
    }
}

fn lua_text_value(value: Value) -> mlua::Result<String> {
    match value {
        Value::String(text) => Ok(text.to_str()?.to_string()),
        Value::Integer(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Boolean(value) => Ok(value.to_string()),
        _ => Ok(String::new()),
    }
}

fn lua_format_text(lua: &Lua, args: &MultiValue) -> mlua::Result<String> {
    let offset = method_arg_offset(args);
    if args.get(offset).is_none() {
        return Ok(String::new());
    }
    let mut call_args = MultiValue::new();
    for index in offset..args.len() {
        if let Some(value) = args.get(index) {
            call_args.push_back(value.clone());
        }
    }
    let string_table = lua.globals().get::<Table>("string")?;
    let format = string_table.get::<Function>("format")?;
    lua_text_value(format.call::<Value>(call_args)?)
}

#[inline(always)]
fn make_color_table(lua: &Lua, rgba: [f32; 4]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, rgba[0])?;
    table.raw_set(2, rgba[1])?;
    table.raw_set(3, rgba[2])?;
    table.raw_set(4, rgba[3])?;
    Ok(table)
}

fn create_color_constants_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (name, text) in [
        ("Black", "0,0,0,1"),
        ("Blue", "#00aeef"),
        ("Green", "#39b54a"),
        ("HoloBlue", "#33B5E5"),
        ("HoloDarkBlue", "#0099CC"),
        ("HoloDarkGreen", "#669900"),
        ("HoloDarkOrange", "#FF8800"),
        ("HoloDarkPurple", "#9933CC"),
        ("HoloDarkRed", "#CC0000"),
        ("HoloGreen", "#99CC00"),
        ("HoloOrange", "#FFBB33"),
        ("HoloPurple", "#AA66CC"),
        ("HoloRed", "#FF4444"),
        ("Invisible", "1,1,1,0"),
        ("Orange", "#f7941d"),
        ("Outline", "0,0,0,0.5"),
        ("Pink", "1,0.75,0.8,1"),
        ("Purple", "#92278f"),
        ("Red", "#ed1c24"),
        ("Stealth", "0,0,0,0"),
        ("White", "1,1,1,1"),
        ("Yellow", "#fff200"),
    ] {
        table.set(
            name,
            make_color_table(lua, parse_color_text(text).unwrap_or([1.0, 1.0, 1.0, 1.0]))?,
        )?;
    }
    table.set(
        "Alpha",
        lua.create_function(|lua, (color, alpha): (Value, f32)| {
            let mut color = read_color_value(color).unwrap_or([1.0, 1.0, 1.0, 1.0]);
            color[3] = alpha;
            make_color_table(lua, color)
        })?,
    )?;
    let table_for_call = table.clone();
    let mt = lua.create_table()?;
    mt.set(
        "__call",
        lua.create_function(move |lua, (_self, value): (Table, Value)| {
            let Some(name) = read_string(value) else {
                return Ok(Value::Nil);
            };
            let existing = table_for_call.get::<Value>(name.as_str())?;
            if !matches!(existing, Value::Nil) {
                return Ok(existing);
            }
            Ok(parse_color_text(&name)
                .map(|color| make_color_table(lua, color).map(Value::Table))
                .transpose()?
                .unwrap_or(Value::Nil))
        })?,
    )?;
    let _ = table.set_metatable(Some(mt));
    Ok(table)
}

#[inline(always)]
fn table_color(table: &Table) -> Option<[f32; 4]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<Option<f32>>(4).ok()?.unwrap_or(1.0),
    ])
}

fn parse_color_text(text: &str) -> Option<[f32; 4]> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(hex) = text.strip_prefix('#') {
        if matches!(hex.len(), 6 | 8) {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
            let a = if hex.len() == 8 {
                u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0
            } else {
                1.0
            };
            return Some([r, g, b, a]);
        }
    }
    let parts = text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [r, g, b] => Some([
            r.parse::<f32>().ok()?,
            g.parse::<f32>().ok()?,
            b.parse::<f32>().ok()?,
            1.0,
        ]),
        [r, g, b, a] => Some([
            r.parse::<f32>().ok()?,
            g.parse::<f32>().ok()?,
            b.parse::<f32>().ok()?,
            a.parse::<f32>().ok()?,
        ]),
        _ => None,
    }
}

#[inline(always)]
fn read_color_value(value: Value) -> Option<[f32; 4]> {
    match value {
        Value::Table(table) => table_color(&table),
        Value::String(text) => Some(parse_color_text(&text.to_str().ok()?).unwrap_or([1.0; 4])),
        _ => None,
    }
}

fn read_color_call(args: &MultiValue) -> Option<[f32; 4]> {
    if let Some(color) = args.front().cloned().and_then(read_color_value) {
        return Some(color);
    }
    let r = args.front().cloned().and_then(read_f32)?;
    let g = args.get(1).cloned().and_then(read_f32)?;
    let b = args.get(2).cloned().and_then(read_f32)?;
    let a = args.get(3).cloned().and_then(read_f32).unwrap_or(1.0);
    Some([r, g, b, a])
}

#[inline(always)]
fn method_arg(args: &MultiValue, index: usize) -> Option<&Value> {
    let offset = method_arg_offset(args);
    args.get(offset + index)
}

#[inline(always)]
fn method_arg_offset(args: &MultiValue) -> usize {
    usize::from(matches!(args.front(), Some(Value::Table(_))))
}

fn song_group_name(song_dir: &Path) -> String {
    song_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn song_music_path(song_dir: &Path) -> Option<PathBuf> {
    song_named_file_path(
        song_dir,
        &["music", "song", "audio"],
        is_song_lua_audio_path,
    )
    .or_else(|| song_first_file_path(song_dir, is_song_lua_audio_path))
}

fn song_named_image_path(song_dir: &Path, stems: &[&str]) -> Option<PathBuf> {
    song_named_file_path(song_dir, stems, is_song_lua_image_path)
}

fn song_simfile_path(song_dir: &Path) -> Option<PathBuf> {
    song_first_file_path(song_dir, is_song_lua_simfile_path)
}

fn song_named_file_path(
    song_dir: &Path,
    stems: &[&str],
    predicate: fn(&Path) -> bool,
) -> Option<PathBuf> {
    let files = song_dir_files(song_dir);
    for stem in stems {
        if let Some(path) = files
            .iter()
            .find(|path| predicate(path) && path_stem_eq(path, stem))
        {
            return Some(path.clone());
        }
    }
    for stem in stems {
        if let Some(path) = files.iter().find(|path| {
            predicate(path)
                && path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.to_ascii_lowercase().contains(stem))
        }) {
            return Some(path.clone());
        }
    }
    None
}

fn song_first_file_path(song_dir: &Path, predicate: fn(&Path) -> bool) -> Option<PathBuf> {
    song_dir_files(song_dir)
        .into_iter()
        .find(|path| predicate(path))
}

fn song_dir_files(song_dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(song_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort_by_key(|path| file_path_string(path));
    files
}

fn path_stem_eq(path: &Path, stem: &str) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(stem))
}

#[inline(always)]
fn read_u32_value(value: Value) -> Option<u32> {
    match value {
        Value::Integer(value) if value >= 0 => u32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() && value >= 0.0 && value.fract() == 0.0 => {
            u32::try_from(value as u64).ok()
        }
        _ => None,
    }
}

#[inline(always)]
fn read_i32_value(value: Value) -> Option<i32> {
    match value {
        Value::Integer(value) => Some(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
        Value::Number(value) if value.is_finite() => Some(
            value
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        ),
        Value::String(text) => text
            .to_str()
            .ok()?
            .trim()
            .parse::<i32>()
            .ok()
            .or_else(|| read_f32(Value::String(text)).map(|value| value.round() as i32)),
        _ => None,
    }
}

#[inline(always)]
fn read_player(value: Value) -> Option<u8> {
    match value {
        Value::Integer(value) if (1..=2).contains(&value) => Some(value as u8),
        Value::Number(value) if (1.0..=2.0).contains(&value) => Some(value as u8),
        _ => None,
    }
}

#[inline(always)]
fn read_span_mode(value: Value) -> Option<SongLuaSpanMode> {
    let text = read_string(value)?;
    if text.eq_ignore_ascii_case("len") {
        Some(SongLuaSpanMode::Len)
    } else if text.eq_ignore_ascii_case("end") {
        Some(SongLuaSpanMode::End)
    } else {
        None
    }
}

#[inline(always)]
fn read_easing_name(value: Value, easing_names: &HashMap<*const c_void, String>) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_str().ok()?.to_string()),
        Value::Function(function) => easing_names.get(&function.to_pointer()).cloned(),
        _ => None,
    }
}

#[inline(always)]
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

fn lua_values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::Integer(left), Value::Integer(right)) => left == right,
        (Value::Integer(left), Value::Number(right)) => (*left as f64) == *right,
        (Value::Number(left), Value::Integer(right)) => *left == (*right as f64),
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::String(left), Value::String(right)) => left
            .to_str()
            .ok()
            .zip(right.to_str().ok())
            .is_some_and(|(left, right)| left == right),
        (Value::Table(left), Value::Table(right)) => left.to_pointer() == right.to_pointer(),
        _ => false,
    }
}

#[inline(always)]
fn player_index_from_value(value: &Value) -> Option<usize> {
    match value {
        Value::Integer(value) => usize::try_from(*value).ok().filter(|v| *v < LUA_PLAYERS),
        Value::Number(value) => {
            if *value >= 0.0 && *value < LUA_PLAYERS as f64 {
                Some(*value as usize)
            } else {
                None
            }
        }
        Value::String(text) => match text.to_str().ok()?.as_ref() {
            "PlayerNumber_P1" => Some(0),
            "PlayerNumber_P2" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

#[inline(always)]
fn player_number_name(player: usize) -> &'static str {
    match player {
        0 => "PlayerNumber_P1",
        1 => "PlayerNumber_P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}

#[inline(always)]
fn song_dir_string(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if !text.ends_with('/') {
        text.push('/');
    }
    text
}

#[inline(always)]
fn file_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[inline(always)]
fn is_song_lua_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "qoi" | "tif" | "tiff"
            )
        })
}

#[inline(always)]
fn is_song_lua_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mp4" | "avi" | "webm" | "mov" | "mkv" | "mpg" | "mpeg" | "ogv"
            )
        })
}

#[inline(always)]
fn is_song_lua_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "ogg" | "mp3" | "wav" | "flac" | "opus" | "m4a" | "aac"
            )
        })
}

#[inline(always)]
fn is_song_lua_simfile_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "sm" | "ssc"))
}

#[inline(always)]
fn is_song_lua_media_path(path: &Path) -> bool {
    is_song_lua_image_path(path) || is_song_lua_video_path(path) || is_song_lua_audio_path(path)
}

#[inline(always)]
fn mod_window_cmp(left: &SongLuaModWindow, right: &SongLuaModWindow) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
        .then_with(|| left.mods.cmp(&right.mods))
}

#[inline(always)]
fn ease_window_cmp(left: &SongLuaEaseWindow, right: &SongLuaEaseWindow) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
}

#[inline(always)]
fn message_event_cmp(
    left: &SongLuaMessageEvent,
    right: &SongLuaMessageEvent,
) -> std::cmp::Ordering {
    left.beat
        .total_cmp(&right.beat)
        .then_with(|| left.message.cmp(&right.message))
}

#[cfg(test)]
mod tests {
    use super::{
        EffectClock, EffectMode, SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget,
        SongLuaOverlayBlendMode, SongLuaOverlayKind, SongLuaPlayerContext, SongLuaProxyTarget,
        SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit, compile_song_lua, file_path_string,
    };
    use crate::engine::present::actors::TextAlign;
    use chrono::{Datelike, Local};
    use std::fs;
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("deadsync-song-lua-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Test Song")).unwrap();
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
        assert_eq!(compiled.info.unsupported_function_actions, 1);
        assert_eq!(compiled.info.unsupported_perframes, 1);
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

        let compiled = compile_song_lua(
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
            p:rotationz(beat + seconds + pos)
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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
                .all(|window| window.from == 9.0 && window.to == 9.0)
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Perframe Side Effects"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_perframes, 0);
        assert!(compiled.eases.is_empty());
        assert!(compiled.overlay_eases.is_empty());
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Startup Command Song"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "StartupReady");
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(&entry, &context).unwrap();
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

        let compiled = compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "lambda");
    }

    #[test]
    fn compile_song_lua_exposes_noteskin_helpers() {
        let song_dir = test_dir("noteskin-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local x = NOTESKIN:GetMetricF("", "TapNoteNoteColorTextureCoordSpacingX")
local y = NOTESKIN:GetMetricFForNoteSkin("", "TapNoteNoteColorTextureCoordSpacingY", "cyber")
local vivid = NOTESKIN:GetMetricBForNoteSkin("", "TapNoteAnimationIsVivid", "cyber")
local path = NOTESKIN:GetPathForNoteSkin("Down", "Tap Explosion Bright W1", "cyber")
local actor = NOTESKIN:LoadActorForNoteSkin("Down", "Tap Explosion Bright W1", "cyber")

if math.abs(x - 0.125) > 0.0001 then
    error("unexpected noteskin metric x: " .. tostring(x))
end
if math.abs(y - 0.0) > 0.0001 then
    error("unexpected noteskin metric y: " .. tostring(y))
end
if vivid ~= false then
    error("unexpected noteskin vivid flag: " .. tostring(vivid))
end
if type(path) ~= "string" or path == "" then
    error("expected noteskin path")
end
if type(actor) ~= "table" then
    error("expected noteskin actor table")
end

mod_actions = {
    {4, tostring(vivid) .. ":" .. tostring(x), true},
}

return Def.ActorFrame{
    actor..{
        Name="NoteskinExplosion",
    },
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Noteskin Helpers");
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

        let compiled = compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "false:0.125");
        assert!(
            compiled.overlays.iter().any(|overlay| {
                overlay.name.as_deref() == Some("NoteskinExplosion")
                    && matches!(overlay.kind, SongLuaOverlayKind::Sprite { .. })
            }),
            "noteskin actor should materialize as a sprite overlay when it resolves to an image"
        );
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Noteskin Concat"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ConcatNoteskin");
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "BitmapText")).unwrap();
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText Style Shims"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(
            compiled.overlays[0].initial_state.wrap_width_pixels,
            Some(88)
        );
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Color Helpers"),
        )
        .unwrap();
        assert!(compiled.overlays.is_empty());
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

        let compiled = compile_song_lua(
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
        let compiled = compile_song_lua(&entry, &context).unwrap();

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

        let compiled = compile_song_lua(
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "IValues")).unwrap();
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Metrics"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "theme-metrics-ok");
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
        let compiled = compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "false:false:PlayerNumber_P1:dance:1:1:CoinMode_Free:Premium_Off:Challenge:true:Common:true:Player 1:0"
        );
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

PREFSMAN:SetPreference("Theme", "Ignored")
PROFILEMAN:SetStatsPrefix("Stats")
PROFILEMAN:SaveMachineProfile()
GAMESTATE:SaveProfiles()
SONGMAN:SetPreferredSongs("Favorites.txt", true)

local song = GAMESTATE:GetCurrentSong()
local steps = GAMESTATE:GetCurrentSteps(PLAYER_1)
local all = SONGMAN:GetAllSongs()
local groups = SONGMAN:GetSongGroupNames()
local found = SONGMAN:FindSong(song:GetSongDir())
local pss = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local played = STATSMAN:GetPlayedStageStats(1):GetPlayerStageStats(PLAYER_2)
local highscore = pss:GetHighScore()
local machine_scores = PROFILEMAN:GetMachineProfile():GetHighScoreList(song, steps):GetHighScores()

mod_actions = {
    {
        1,
        string.format(
            "%d:%s:%s:%.0f:%s:%s:%d:%d:%s:%s:%.0f:%d:%d:%s:%s:%d",
            GAMESTATE:GetNumPlayersEnabled(),
            THEME:GetCurThemeName(),
            THEME:GetThemeDisplayName(),
            GetTimeSinceStart(),
            tostring(HolidayCheer()),
            tostring(SONGMAN:GetPreferredSortSongs()),
            #all,
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Manager Compat"),
        )
        .unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "2:Simply Love:Simply Love:0:false:true:1:1:Theme Manager Compat:true:0:1:0:Grade_Tier07:Player:1"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
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

        let compiled = compile_song_lua(
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
        let compiled = compile_song_lua(&entry, &context).unwrap();

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

        let compiled = compile_song_lua(
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
            tostring(top:GetNextCourseSong() == nil)
        ),
        true,
    },
}

top:Cancel():Finish():begin_backing_out()

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
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

        compile_song_lua(
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Song"),
        )
        .unwrap();
        assert_eq!(compiled.beat_mods.len(), 1);
        assert_eq!(compiled.beat_mods[0].start, 4.0);
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Overlay")).unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let overlay = &compiled.overlays[0];
        assert_eq!(overlay.parent_index, None);
        assert!(matches!(
            overlay.kind,
            SongLuaOverlayKind::Sprite { ref texture_path }
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Message Error"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert!(compiled.overlays[0].message_commands.is_empty());
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
        MESSAGEMAN:Broadcast("ProxyStart")
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

        let compiled = compile_song_lua(
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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
        let compiled = compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "854:480:true:true");
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
        string.format("%d:%d:%d", Year(), MonthOfYear(), DayOfMonth()),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
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
        assert_eq!(parts.len(), 3);
        let now = Local::now();
        assert_eq!(parts[0], now.year());
        assert_eq!(parts[1], now.month0() as i32);
        assert_eq!(parts[2], now.day() as i32);
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
        let compiled = compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Display BPMs:120:180:150:200");
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
        image::RgbaImage::new(320, 240)
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
        let compiled = compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "Song Metadata|Song Metadata||Pack A|true|true|true|true|true|1|true|true|true|6|10|123.4|0|90|180|150"
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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
            self:play():pause():stop():load("rain.ogg"):volume(0.5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Save Your Tears Compat"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "6:1x, Overhead");
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
        let compiled = compile_song_lua(&entry, &context).unwrap();

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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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
        let compiled = compile_song_lua(&entry, &context).unwrap();

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
        let compiled = compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Medium");
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "LoadActor Video Media"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:true");
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

        let compiled = compile_song_lua(
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
                    tostring(self.File == "clip.ogg"),
                    tostring(self:GetTexture() == nil)
                ), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "LoadActor Audio Media"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:true");
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Hibernate"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "0.25");
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Additive Transforms"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "15:17:10:20:35:135");
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
            self:setstate(2):play():pause():play():diffusealpha(0.2)
            mod_actions = {
                {1, string.format("%.2f:%d:%d", self:GetDiffuseAlpha(), self:GetNumStates(), texture:GetNumFrames()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
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
        assert_eq!(state.sprite_state_index, Some(2));
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
    Def.BitmapText{
        Font="Common Normal",
        Text="TEXT",
        OnCommand=function(self)
            self:strokecolor(color("0.2,0.3,0.4,0.5"))
                :max_dimension_use_zoom(true)
                :textglowmode("Both")
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite Load Text Compat"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 2);
        assert_eq!(compiled.messages[0].message, "true:true:2:1.00:20:20");
        assert_eq!(compiled.messages[1].message, "0.2:0.3:0.4:0.5:true");
        assert_eq!(compiled.overlays.len(), 2);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::Sprite { ref texture_path }
                if texture_path.ends_with("sheet 2x2.png")
        ));
        assert_eq!(
            compiled.overlays[0].initial_state.sprite_state_index,
            Some(2)
        );
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::BitmapText {
                stroke_color: Some([0.2, 0.3, 0.4, 0.5]),
                ..
            }
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
            crate::engine::present::anim::EffectMode::GlowShift
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Vertex Diffuse Shims"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ok");
        assert_eq!(compiled.overlays.len(), 1);
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText Layout Methods"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);

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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BaseZoom Axis"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.basezoom, 2.0);
        assert_eq!(compiled.overlays[0].initial_state.basezoom_x, 3.0);
        assert_eq!(compiled.overlays[0].initial_state.basezoom_y, 4.0);
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor State Getters"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "10:20:3:3:4:5:0:1:8:0.2:0.6:0.1:0.4:false:8:4:2:0:0"
        );
        assert_eq!(compiled.overlays.len(), 1);
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "BaseZoom Z")).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ok");
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Math Round")).unwrap();
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Xero")).unwrap();
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Fileman")).unwrap();
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
local child = Def.ActorFrame{Name="child"}
local root = Def.ActorFrame{
    child,
    Def.ActorFrame{
        InitCommand=function(self)
            local getchildat = ActorFrame.GetChildAt or function(actor, index)
                local res = nil
                actor:RunCommandsOnChildren(function(candidate)
                    if candidate:GetParent() == actor and res == nil then
                        index = index - 1
                        if index == 0 then
                            res = candidate
                        end
                    end
                end)
                return res
            end
            local picked = getchildat(self, 1)
            mod_actions = {
                {1, string.format("%s:%s", tostring(ActorFrame.fardistz ~= nil), picked and picked:GetName() or "nil"), true},
            }
        end,
    },
}

return root
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "ActorFrame Class"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:nil");
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Actor Compat Methods"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:true:2:0");
        assert_eq!(compiled.overlays.len(), 1);
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
            self:SetTextureFiltering(false):zwrite(true):ztest(true):ztestmode("WriteOnFail"):draworder(100)
            self:zbuffer(true):zbias(2):backfacecull(true):cullmode("CullMode_Back")
            self:aux(self:getaux() + 0.75)
            mod_actions = {
                {1, string.format("%.2f", self:getaux()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Aux Compat Shims"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1.00");
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.draw_order, 100);
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(&entry, &context).unwrap();
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "NoteField Column API"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "-96:0");
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

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Player Options Getters"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1.00:0.25:true");
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

        let compiled = compile_song_lua(
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
            SongLuaOverlayKind::Sprite { ref texture_path }
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local theme_info = IniFile.ReadFile(THEME:GetCurrentThemeDirectory() .. "ThemeInfo.ini").ThemeInfo
assert(theme_info.DisplayName == "Simply Love")
local encoded = JsonEncode({a=1, b="two words", nested={true,false}})
local decoded = JsonDecode(encoded)
assert(decoded.a == 1)
assert(BinaryToHex(CRYPTMAN:SHA1String("chart")) == string.rep("0", 40))
assert(NETWORK:EncodeQueryParameters({b="two words", a=1}) == "a=1&b=two%20words")
assert(NETWORK:HttpRequest{url="https://example.invalid"}.body == "")
local ws = NETWORK:WebSocket{url="wss://example.invalid"}
ws:Send(JsonEncode({uuid=CRYPTMAN:GenerateRandomUUID()}))
ws:Close()
local file = RageFileUtil:CreateRageFile()
assert(file:Open("Save/Offline.json", 2))
assert(file:Write(encoded))
assert(file:Read() == "")
file:Close()
file:destroy()
assert(FILEMAN:Unzip("archive.zip", "Songs/Pack") == false)
assert(GetTimingWindow(2) > GetTimingWindow(1))
assert(GetWorstJudgment({{0, GetTimingWindow(3)}}) == 3)
local ex, points, possible = CalculateExScore(PLAYER_1)
assert(ex == 0 and points == 0 and possible == 0)

mod_actions = {
    {1, function()
        NETWORK:HttpRequest{url="https://example.invalid", body=JsonEncode(decoded)}
        GAMESTATE:JoinPlayer(PLAYER_1)
        FILEMAN:CreateDir("Save")
        FILEMAN:Remove("Save/Offline.json")
        FILEMAN:FlushDirCache()
        assert(IsHumanPlayer(PLAYER_1) == GAMESTATE:IsSideJoined(PLAYER_1))
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Offline Theme Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
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
assert(profile:GetCaloriesBurnedToday() == 0)
assert(profile:GetNumTotalSongsPlayed() == 0)
assert(profile:GetTotalSessions() == 0)
assert(profile:GetIgnoreStepCountCalories() == false)
assert(profile:CalculateCaloriesFromHeartRate(120, 60) == 0)
profile:AddCaloriesToDailyTotal(0)
profile:SetLastUsedHighScoreName("AAA")
assert(profile:GetLastUsedHighScoreName() == "AAA")

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

        let compiled = compile_song_lua(
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
local steps = pss:GetPlayedSteps()[1]
assert(steps:GetMeter() >= 0)
assert(steps:GetDifficulty() ~= nil)
assert(pss:GetMachineHighScoreIndex() == -1)
assert(pss:GetPersonalHighScoreIndex() == -1)
assert(pss:IsDisqualified() == false)
assert(pss:FullComboOfScore(0) == false)
assert(pss:GetCurrentPossibleDancePoints() == 1)
assert(pss:GetCurrentCombo() == 0)
assert(pss:GetCurrentMissCombo() == 0)

local highscore = pss:GetHighScore()
assert(highscore:GetHoldNoteScore("HoldNoteScore_Held") == 0)
assert(highscore:GetMaxCombo() == 0)
assert(highscore:GetSurvivalSeconds() == 0)
assert(highscore:GetStageAward() == "StageAward_None")
assert(STATSMAN:GetCurStageStats():GaveUp() == false)

mod_actions = {
    {1, function()
        local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
        stats:SetScore(12)
        stats:FailPlayer()
        assert(stats:GetFailed() == true)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Stage Stat High Score Helpers"),
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
            compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "AFT Draw")).unwrap();
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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

        let compiled = compile_song_lua(
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
    fn compile_song_lua_supports_spooky_sample_if_present() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../lua-songs/[07] Spooky (SM) [Scrypts]");
        let entry = root.join("lua/default.lua");
        if !entry.is_file() {
            return;
        }

        let mut context = SongLuaCompileContext::new(&root, "Spooky");
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

        let compiled = compile_song_lua(&entry, &context).unwrap();
        assert!(!compiled.beat_mods.is_empty());
        assert_eq!(compiled.messages.len(), 2);
        assert_eq!(compiled.overlays.len(), 3);
        assert!(compiled.eases.len() >= 40);
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
    fn compile_song_lua_supports_media_offline_sample_if_present() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../lua-songs/[10] media offline (SM) [Snap]");
        let entry = root.join("lua/_script.lua");
        if !entry.is_file() {
            return;
        }

        let mut context = SongLuaCompileContext::new(&root, "media offline");
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

        let compiled = compile_song_lua(&entry, &context).unwrap();
        assert!(!compiled.time_mods.is_empty());
        assert!(
            compiled.eases.iter().any(
                |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "tiny")
            )
        );
    }

    #[test]
    fn compile_song_lua_supports_step_your_game_up_sample_if_present() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../lua-songs/Step Your Game Up (Director's Cut)");
        let entry = root.join("lua/default.lua");
        if !entry.is_file() {
            return;
        }

        let mut context = SongLuaCompileContext::new(&root, "Step Your Game Up");
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

        let compiled = compile_song_lua(&entry, &context).unwrap();
        assert!(!compiled.beat_mods.is_empty());
        assert!(!compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_supports_kenpo_sample_if_present() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../lua-songs/[11] KENPO SAITO (DX) [Scrypts]");
        let entry = root.join("template/main.lua");
        if !entry.is_file() {
            return;
        }

        let mut context = SongLuaCompileContext::new(&root, "KENPO SAITO");
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

        let compiled = compile_song_lua(&entry, &context).unwrap();
        assert!(compiled.hidden_players[0] || compiled.hidden_players[1]);
        assert!(
            compiled
                .overlays
                .iter()
                .any(|overlay| matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture))
        );
        assert!(
            compiled
                .overlays
                .iter()
                .any(|overlay| matches!(overlay.kind, SongLuaOverlayKind::AftSprite { .. }))
        );
    }

    #[test]
    fn compile_song_lua_supports_vector_field_sample_if_present() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lua-songs/Vector Field");
        let entry = root.join("template/main.lua");
        if !entry.is_file() {
            return;
        }

        let compiled =
            compile_song_lua(&entry, &SongLuaCompileContext::new(&root, "Vector Field")).unwrap();
        assert!(!compiled.overlays.is_empty());
    }
}
