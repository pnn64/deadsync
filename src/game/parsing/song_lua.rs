use crate::engine::present::anim::EffectMode;
use image::image_dimensions;
use log::debug;
use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const LUA_PLAYERS: usize = 2;
const SONG_LUA_NOTE_COLUMNS: usize = 4;
const SONG_LUA_PRODUCT_FAMILY: &str = "ITGmania";
const SONG_LUA_PRODUCT_ID: &str = "ITGmania";
const SONG_LUA_PRODUCT_VERSION: &str = "1.2.0";
const THEME_RECEPTOR_Y_STD: f32 = -125.0;
const THEME_RECEPTOR_Y_REV: f32 = 145.0;
const SONG_LUA_COLUMN_X: [f32; SONG_LUA_NOTE_COLUMNS] = [-96.0, -32.0, 32.0, 96.0];
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

#[derive(Debug, Clone)]
pub struct SongLuaCompileContext {
    pub song_dir: PathBuf,
    pub main_title: String,
    pub song_display_bpms: [f32; 2],
    pub song_music_rate: f32,
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
    PlayerRotationZ,
    PlayerRotationY,
    PlayerSkewX,
    PlayerZoomX,
    PlayerZoomY,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaProxyTarget {
    Player { player_index: usize },
    NoteField { player_index: usize },
    Judgment { player_index: usize },
    Combo { player_index: usize },
    Underlay,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaOverlayBlendMode {
    Alpha,
    Add,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SongLuaOverlayKind {
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
    },
    BitmapText {
        font_name: &'static str,
        font_path: PathBuf,
        text: String,
        stroke_color: Option<[f32; 4]>,
    },
    Quad,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayState {
    pub x: f32,
    pub y: f32,
    pub diffuse: [f32; 4],
    pub visible: bool,
    pub cropleft: f32,
    pub cropright: f32,
    pub croptop: f32,
    pub cropbottom: f32,
    pub zoom: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub basezoom: f32,
    pub basezoom_x: f32,
    pub basezoom_y: f32,
    pub rot_x_deg: f32,
    pub rot_y_deg: f32,
    pub rot_z_deg: f32,
    pub blend: SongLuaOverlayBlendMode,
    pub vibrate: bool,
    pub effect_magnitude: [f32; 3],
    pub effect_mode: EffectMode,
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub effect_period: f32,
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
            diffuse: [1.0, 1.0, 1.0, 1.0],
            visible: true,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            zoom: 1.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            basezoom: 1.0,
            basezoom_x: 1.0,
            basezoom_y: 1.0,
            rot_x_deg: 0.0,
            rot_y_deg: 0.0,
            rot_z_deg: 0.0,
            blend: SongLuaOverlayBlendMode::Alpha,
            vibrate: false,
            effect_magnitude: [0.0, 0.0, 0.0],
            effect_mode: EffectMode::None,
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            effect_period: 1.0,
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
    pub diffuse: Option<[f32; 4]>,
    pub visible: Option<bool>,
    pub cropleft: Option<f32>,
    pub cropright: Option<f32>,
    pub croptop: Option<f32>,
    pub cropbottom: Option<f32>,
    pub zoom: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub basezoom: Option<f32>,
    pub basezoom_x: Option<f32>,
    pub basezoom_y: Option<f32>,
    pub rot_x_deg: Option<f32>,
    pub rot_y_deg: Option<f32>,
    pub rot_z_deg: Option<f32>,
    pub blend: Option<SongLuaOverlayBlendMode>,
    pub vibrate: Option<bool>,
    pub effect_magnitude: Option<[f32; 3]>,
    pub effect_mode: Option<EffectMode>,
    pub effect_color1: Option<[f32; 4]>,
    pub effect_color2: Option<[f32; 4]>,
    pub effect_period: Option<f32>,
    pub custom_texture_rect: Option<[f32; 4]>,
    pub texcoord_velocity: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
    pub stretch_rect: Option<[f32; 4]>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayMessageCommand {
    pub message: String,
    pub blocks: Vec<SongLuaOverlayCommandBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayActor {
    pub kind: SongLuaOverlayKind,
    pub name: Option<String>,
    pub parent_index: Option<usize>,
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongLuaCompileInfo {
    pub unsupported_perframes: usize,
    pub unsupported_function_eases: usize,
    pub unsupported_function_actions: usize,
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
    let mut overlay_trigger_counter = 0usize;

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
        out.info.unsupported_perframes += table_len(
            prefix_globals
                .get::<Option<Table>>("perframes")
                .map_err(|err| err.to_string())?,
        )?;
        let fn_actions = read_actions(
            &lua,
            prefix_globals
                .get::<Option<Table>>("actions")
                .map_err(|err| err.to_string())?,
            &mut overlays,
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
    out.info.unsupported_perframes += table_len(
        globals
            .get::<Option<Table>>("mod_perframes")
            .map_err(|err| err.to_string())?,
    )?;
    let global_fn_actions = read_actions(
        &lua,
        globals
            .get::<Option<Table>>("mod_actions")
            .map_err(|err| err.to_string())?,
        &mut overlays,
        &mut out.messages,
        &mut overlay_trigger_counter,
    )?;
    out.info.unsupported_function_actions += global_fn_actions;
    out.overlays = overlays.into_iter().map(|overlay| overlay.actor).collect();
    out.hidden_players = std::array::from_fn(|player| {
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
    globals.set("unpack", table.get::<Value>("unpack")?)?;
    globals.set("Trace", lua.create_function(|_, _msg: String| Ok(()))?)?;
    globals.set("debug", create_debug_table(lua)?)?;
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
    globals.set("FILEMAN", fileman)?;
    Ok(())
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
    mt.set(
        "__newindex",
        lua.create_function(move |_, (_self, key, value): (Table, Value, Value)| {
            let target: Table = proxy_for_newindex.get("__songlua_env_target")?;
            target.set(key, value)?;
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
        "ASPECT_SCALE_FACTOR",
        screen_width / (640.0 * (screen_height / 480.0)),
    )?;
    globals.set(
        "scale",
        lua.create_function(
            |_, (value, from_low, from_high, to_low, to_high): (f32, f32, f32, f32, f32)| {
                let span = from_high - from_low;
                if span.abs() <= f32::EPSILON {
                    return Ok(to_low);
                }
                Ok((value - from_low) / span * (to_high - to_low) + to_low)
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
            } else {
                Ok(Value::Nil)
            }
        })?,
    )?;
    globals.set("PREFSMAN", prefsmgr)?;
    globals.set("DISPLAY", create_display_table(lua, context)?)?;
    globals.set("THEME", create_theme_table(lua)?)?;
    globals.set("NOTESKIN", create_noteskin_table(lua, context)?)?;
    globals.set("ArrowEffects", create_arrow_effects_table(lua)?)?;

    let song = create_song_table(lua, context)?;
    let players = create_player_tables(lua, context)?;
    let song_options = create_song_options_table(lua, context.song_music_rate)?;
    let gamestate = lua.create_table()?;
    let enabled_players = create_enabled_players_table(lua, context.players.clone())?;
    let human_players = enabled_players.clone();
    let song_clone = song.clone();
    gamestate.set(
        "GetCurrentSong",
        lua.create_function(move |_, _args: MultiValue| Ok(song_clone.clone()))?,
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
    let current_steps = players.steps.clone();
    gamestate.set(
        "GetCurrentSteps",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(current_steps[player].clone()))
        })?,
    )?;
    gamestate.set(
        "GetSongBeat",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    gamestate.set(
        "GetCurMusicSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    let song_position = create_song_position_table(lua)?;
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
    let top_screen_table = top_screen.top_screen.clone();
    screenman.set(
        "GetTopScreen",
        lua.create_function(move |_, _args: MultiValue| Ok(top_screen_table.clone()))?,
    )?;
    screenman.set(
        "SystemMessage",
        lua.create_function(|_, _args: MultiValue| Ok(()))?,
    )?;
    globals.set("SCREENMAN", screenman)?;

    let messageman = lua.create_table()?;
    messageman.set(
        "Broadcast",
        lua.create_function(|_, _args: MultiValue| Ok(()))?,
    )?;
    globals.set("MESSAGEMAN", messageman)?;
    Ok(())
}

fn create_difficulty_table(lua: &Lua) -> mlua::Result<Table> {
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
        table.raw_set(idx + 1, difficulty.sm_name())?;
    }
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
    Ok(spline)
}

fn create_top_screen_table(
    lua: &Lua,
    players: [SongLuaPlayerContext; LUA_PLAYERS],
) -> mlua::Result<TopScreenLuaTables> {
    let top_screen = create_dummy_actor(lua, "TopScreen")?;
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
    Ok(TopScreenLuaTables {
        top_screen,
        players: player_actors,
    })
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
        )?,
        create_steps_table(
            lua,
            context.players[1].difficulty,
            context.players[1].display_bpms,
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
    let get_metric = lua.create_function(|_, args: MultiValue| {
        let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
            return Ok(Value::Nil);
        };
        let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
            return Ok(Value::Nil);
        };
        Ok(theme_metric(&group, &name).map_or(Value::Nil, |value| Value::Number(value as f64)))
    })?;
    theme.set("GetMetric", get_metric.clone())?;
    theme.set("GetMetricF", get_metric)?;
    Ok(theme)
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

#[inline(always)]
fn theme_metric(group: &str, name: &str) -> Option<f32> {
    if !group.eq_ignore_ascii_case("Player") {
        return None;
    }
    if name.eq_ignore_ascii_case("ReceptorArrowsYStandard") {
        Some(THEME_RECEPTOR_Y_STD)
    } else if name.eq_ignore_ascii_case("ReceptorArrowsYReverse") {
        Some(THEME_RECEPTOR_Y_REV)
    } else {
        None
    }
}

fn create_player_state_table(lua: &Lua, player: SongLuaPlayerContext) -> mlua::Result<Table> {
    let options = create_player_options_table(lua, player)?;
    let table = lua.create_table()?;
    table.set(
        "GetPlayerOptions",
        lua.create_function(move |_, _args: MultiValue| Ok(options.clone()))?,
    )?;
    Ok(table)
}

fn create_steps_table(
    lua: &Lua,
    difficulty: SongLuaDifficulty,
    display_bpms: [f32; 2],
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let difficulty = difficulty.sm_name().to_string();
    table.set(
        "GetDifficulty",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&difficulty)?))
        })?,
    )?;
    let display_bpms = create_display_bpms_table(lua, display_bpms)?;
    table.set(
        "GetDisplayBpms",
        lua.create_function(move |_, _args: MultiValue| Ok(display_bpms.clone()))?,
    )?;
    Ok(table)
}

fn create_display_bpms_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, bpms[0])?;
    table.raw_set(2, bpms[1])?;
    Ok(table)
}

fn create_player_options_table(lua: &Lua, player: SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    install_speedmod_method(lua, &table, "CMod", player.speedmod, SongLuaSpeedMod::C)?;
    install_speedmod_method(lua, &table, "MMod", player.speedmod, SongLuaSpeedMod::M)?;
    install_speedmod_method(lua, &table, "AMod", player.speedmod, SongLuaSpeedMod::A)?;
    install_speedmod_method(lua, &table, "XMod", player.speedmod, SongLuaSpeedMod::X)?;
    table.set(
        "Mirror",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    table.set(
        "Left",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    table.set(
        "Right",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    table.set(
        "Skew",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "Tilt",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
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
            let owner = fallback_owner.clone();
            Ok(Value::Function(lua.create_function(
                move |_, _args: MultiValue| Ok(owner.clone()),
            )?))
        })?,
    )?;
    let _ = table.set_metatable(Some(mt));
    Ok(table)
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
    table.set(
        "GetSongDir",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&song_dir)?))
        })?,
    )?;
    let title = context.main_title.clone();
    table.set(
        "GetMainTitle",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&title)?))
        })?,
    )?;
    let display_title = context.main_title.clone();
    table.set(
        "GetDisplayMainTitle",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&display_title)?))
        })?,
    )?;
    let display_bpms = create_display_bpms_table(lua, context.song_display_bpms)?;
    table.set(
        "GetDisplayBpms",
        lua.create_function(move |_, _args: MultiValue| Ok(display_bpms.clone()))?,
    )?;
    let timing = create_timing_table(lua)?;
    table.set(
        "GetTimingData",
        lua.create_function(move |_, _args: MultiValue| Ok(timing.clone()))?,
    )?;
    Ok(table)
}

fn create_timing_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
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

fn create_song_position_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetSongBeat",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
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
    Ok(())
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
                Value::Integer(value) if *value >= 1 => Some(*value as usize),
                Value::Number(value) if value.is_finite() && *value >= 1.0 => Some(*value as usize),
                _ => None,
            }) else {
                return Ok(Value::Nil);
            };
            Ok(match actor.raw_get::<Option<Value>>(index)? {
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
        install_actor_methods(lua, &table)?;
        install_actor_metatable(lua, &table)?;
        reset_actor_capture(lua, &table)?;
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
    let resolved = resolve_script_path(lua, song_dir, path)?;
    if is_song_lua_media_path(&resolved) {
        return Ok(Value::Table(create_media_actor(lua, "Sprite", path)?));
    }
    load_script_file(lua, &resolved, song_dir)?.call::<Value>(())
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
        run_actor_named_command(lua, actor, "OnCommand")?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_startup_commands_for_table(lua, &child)?;
    }
    Ok(())
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

fn run_actor_init_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    run_actor_named_command(lua, actor, "InitCommand")
}

fn run_actor_named_command(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    let Some(command) = actor.get::<Option<Function>>(name)? else {
        return Ok(());
    };
    run_guarded_actor_command(lua, actor, name, &command)
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
    drain_actor_command_queue(lua, actor)
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
    let mut out = Vec::new();
    read_overlay_actors_from_table(lua, root, None, &mut out)?;
    Ok(out)
}

fn read_overlay_actors_from_table(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    out: &mut Vec<OverlayCompileActor>,
) -> Result<(), String> {
    let next_parent_index = if let Some(overlay) = read_overlay_actor(lua, actor, parent_index)? {
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
        read_overlay_actors_from_table(lua, &child, next_parent_index, out)?;
    }
    Ok(())
}

fn read_overlay_actor(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
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
        let blocks = capture_actor_command(lua, actor, name.as_str())?;
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
        if let Some(capture_name) = actor
            .get::<Option<String>>("__songlua_aft_capture_name")
            .map_err(|err| err.to_string())?
        {
            SongLuaOverlayKind::AftSprite { capture_name }
        } else {
            let Some(texture_path) = actor
                .get::<Option<String>>("Texture")
                .map_err(|err| err.to_string())?
                .and_then(|texture| resolve_actor_asset_path(actor, &texture).ok())
            else {
                return Ok(None);
            };
            SongLuaOverlayKind::Sprite { texture_path }
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
            text: actor
                .get::<Option<String>>("Text")
                .map_err(|err| err.to_string())?
                .unwrap_or_default(),
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
                zoom: block
                    .get::<Option<f32>>("zoom")
                    .map_err(|err| err.to_string())?,
                zoom_x: block
                    .get::<Option<f32>>("zoom_x")
                    .map_err(|err| err.to_string())?,
                zoom_y: block
                    .get::<Option<f32>>("zoom_y")
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

fn overlay_state_after_blocks(
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
    if let Some(value) = delta.diffuse {
        state.diffuse = value;
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
    if let Some(value) = delta.zoom {
        state.zoom = value;
    }
    if let Some(value) = delta.zoom_x {
        state.zoom_x = value;
    }
    if let Some(value) = delta.zoom_y {
        state.zoom_y = value;
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
    if let Some(value) = delta.rot_x_deg {
        state.rot_x_deg = value;
    }
    if let Some(value) = delta.rot_y_deg {
        state.rot_y_deg = value;
    }
    if let Some(value) = delta.rot_z_deg {
        state.rot_z_deg = value;
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
    if delta.diffuse.is_some() {
        for i in 0..4 {
            from.diffuse[i] = (to.diffuse[i] - from.diffuse[i]).mul_add(t, from.diffuse[i]);
        }
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
    if delta.zoom.is_some() {
        from.zoom = (to.zoom - from.zoom).mul_add(t, from.zoom);
    }
    if delta.zoom_x.is_some() {
        from.zoom_x = (to.zoom_x - from.zoom_x).mul_add(t, from.zoom_x);
    }
    if delta.zoom_y.is_some() {
        from.zoom_y = (to.zoom_y - from.zoom_y).mul_add(t, from.zoom_y);
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
    if delta.rot_x_deg.is_some() {
        from.rot_x_deg = (to.rot_x_deg - from.rot_x_deg).mul_add(t, from.rot_x_deg);
    }
    if delta.rot_y_deg.is_some() {
        from.rot_y_deg = (to.rot_y_deg - from.rot_y_deg).mul_add(t, from.rot_y_deg);
    }
    if delta.rot_z_deg.is_some() {
        from.rot_z_deg = (to.rot_z_deg - from.rot_z_deg).mul_add(t, from.rot_z_deg);
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
    if delta.effect_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_mode = to.effect_mode;
    }
    from
}

fn parse_overlay_blend_mode(raw: &str) -> Option<SongLuaOverlayBlendMode> {
    if raw.eq_ignore_ascii_case("add") {
        Some(SongLuaOverlayBlendMode::Add)
    } else if raw.eq_ignore_ascii_case("alpha")
        || raw.eq_ignore_ascii_case("normal")
        || raw.eq_ignore_ascii_case("blendmode_normal")
    {
        Some(SongLuaOverlayBlendMode::Alpha)
    } else {
        None
    }
}

fn parse_overlay_effect_mode(raw: &str) -> Option<EffectMode> {
    match raw {
        "none" => Some(EffectMode::None),
        "diffuseshift" => Some(EffectMode::DiffuseShift),
        "spin" => Some(EffectMode::Spin),
        _ => None,
    }
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
    Ok(actor)
}

fn create_media_actor(lua: &Lua, actor_type: &'static str, path: &str) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, actor_type)?;
    actor.set("Texture", path)?;
    Ok(actor)
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
        "SetTexture",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                match args.get(1) {
                    Some(Value::String(texture)) => {
                        actor.set("Texture", texture.to_str()?.to_string())?;
                        actor.set("__songlua_aft_capture_name", Value::Nil)?;
                    }
                    Some(Value::Table(texture)) => {
                        if let Some(texture_path) =
                            texture.get::<Option<String>>("__songlua_texture_path")?
                        {
                            actor.set("Texture", texture_path)?;
                            actor.set("__songlua_aft_capture_name", Value::Nil)?;
                        } else if let Some(capture_name) =
                            texture.get::<Option<String>>("__songlua_aft_capture_name")?
                        {
                            actor.set("__songlua_aft_capture_name", capture_name)?;
                        }
                    }
                    _ => {}
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
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "zoom", value)?;
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
        "cropbottom",
        make_actor_capture_f32_method(lua, actor, "cropbottom", None)?,
    )?;
    actor.set(
        "croptop",
        make_actor_capture_f32_method(lua, actor, "croptop", None)?,
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
        make_actor_capture_f32_method(lua, actor, "zoom_x", Some("zoomx"))?,
    )?;
    actor.set(
        "zoomy",
        make_actor_capture_f32_method(lua, actor, "zoom_y", Some("zoomy"))?,
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
            move |_, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    actor.set("__songlua_state_z", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["skewx", "skewy", "addy", "zoomz"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                let method_name = name.to_string();
                move |lua, _args: MultiValue| {
                    record_probe_method_call(lua, &actor, &method_name)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
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
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(3).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(4).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_vec4(lua, &actor, "custom_texture_rect", rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
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
        "diffuseshift",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let block = actor_current_capture_block(lua, &actor)?;
                block.set("effect_mode", "diffuseshift")?;
                block.set("__songlua_has_changes", true)?;
                actor.set("__songlua_state_effect_mode", "diffuseshift")?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "spin",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let block = actor_current_capture_block(lua, &actor)?;
                block.set("effect_mode", "spin")?;
                block.set("__songlua_has_changes", true)?;
                actor.set("__songlua_state_effect_mode", "spin")?;
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
        "vibrate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "vibrate", true)?;
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
                let block = actor_current_capture_block(lua, &actor)?;
                block.set("effect_mode", "none")?;
                actor.set("__songlua_state_effect_mode", "none")?;
                let block = actor_current_capture_block(lua, &actor)?;
                let value = lua.create_table()?;
                value.raw_set(1, 0.0_f32)?;
                value.raw_set(2, 0.0_f32)?;
                value.raw_set(3, 0.0_f32)?;
                block.set("effect_magnitude", value.clone())?;
                block.set("__songlua_has_changes", true)?;
                actor.set("__songlua_state_effect_magnitude", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in [
        "clearzbuffer",
        "diffuseramp",
        "effectclock",
        "EnableAlphaBuffer",
        "EnableDepthBuffer",
        "EnableFloat",
        "EnablePreserveTexture",
        "Create",
        "fardistz",
        "fov",
        "finishtweening",
        "texturetranslate",
        "vanishpoint",
        "wag",
    ] {
        actor.set(name, make_actor_chain_method(lua, actor)?)?;
    }
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
        "settext",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let text = match args.get(1).cloned() {
                    Some(Value::String(text)) => text.to_str()?.to_string(),
                    Some(Value::Integer(value)) => value.to_string(),
                    Some(Value::Number(value)) => value.to_string(),
                    Some(Value::Boolean(value)) => value.to_string(),
                    _ => String::new(),
                };
                actor.set("Text", text)?;
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
        "GetChild",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                let children = actor_children(lua, &actor)?;
                if let Some(child) = children.get::<Option<Table>>(name.as_str())? {
                    return Ok(Value::Table(child));
                }
                let child = create_named_child_actor(lua, &actor, &name)?;
                children.set(name.as_str(), child.clone())?;
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
            move |lua, _args: MultiValue| actor_children(lua, &actor)
        })?,
    )?;
    actor.set(
        "GetNumChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let mut count = 0_i64;
                for pair in actor_children(lua, &actor)?.pairs::<Value, Value>() {
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
        "GetHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_base_size(&actor)?.1)
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
    if let Some(path) = actor_texture_path(actor)?
        && let Ok((width, height)) = image_dimensions(&path)
    {
        return Ok((width as f32, height as f32));
    }
    Ok((1.0, 1.0))
}

fn create_texture_proxy(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let texture = lua.create_table()?;
    let (screen_width, screen_height) = song_lua_screen_size(lua)?;
    if actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
    {
        if let Some(name) = actor.get::<Option<String>>("Name")? {
            texture.set("__songlua_aft_capture_name", name)?;
        }
        install_texture_proxy_methods(
            lua,
            &texture,
            String::new(),
            screen_width,
            screen_height,
            screen_width,
            screen_height,
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
        path,
        source_width,
        source_height,
        source_width,
        source_height,
    )?;
    Ok(texture)
}

fn install_texture_proxy_methods(
    lua: &Lua,
    texture: &Table,
    path: String,
    source_width: f32,
    source_height: f32,
    texture_width: f32,
    texture_height: f32,
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
    let previous = globals.get::<Value>("__songlua_probe_methods")?;
    let calls = lua.create_table()?;
    globals.set("__songlua_probe_methods", calls.clone())?;
    let result = function.call::<Value>(1.0_f32);
    let methods = probe_call_names(&calls)?;
    let classify = classify_function_ease_probe(&calls);
    globals.set("__songlua_probe_methods", previous)?;
    match result {
        Ok(_) => Ok((classify?, methods)),
        Err(_) => Ok((None, methods)),
    }
}

fn classify_function_ease_probe(calls: &Table) -> mlua::Result<Option<SongLuaEaseTarget>> {
    let mut saw_rotation_z = false;
    let mut saw_rotation_y = false;
    let mut saw_skew_x = false;
    let mut saw_zoom_x = false;
    let mut saw_zoom_y = false;
    for value in calls.sequence_values::<String>() {
        let value = value?;
        let (target_kind, method_name) =
            value.split_once('.').unwrap_or(("player", value.as_str()));
        if !matches!(target_kind, "player" | "notefield") {
            return Ok(None);
        }
        match method_name {
            "rotationz" => saw_rotation_z = true,
            "rotationy" => saw_rotation_y = true,
            "skewx" => saw_skew_x = true,
            "zoomx" => saw_zoom_x = true,
            "zoomy" => saw_zoom_y = true,
            _ => return Ok(None),
        }
    }
    Ok(
        match (
            saw_rotation_z,
            saw_rotation_y,
            saw_skew_x,
            saw_zoom_x,
            saw_zoom_y,
        ) {
            (true, false, false, false, false) => Some(SongLuaEaseTarget::PlayerRotationZ),
            (false, true, false, false, false) => Some(SongLuaEaseTarget::PlayerRotationY),
            (false, false, true, false, false) => Some(SongLuaEaseTarget::PlayerSkewX),
            (false, false, false, true, false) => Some(SongLuaEaseTarget::PlayerZoomX),
            (false, false, false, false, true) => Some(SongLuaEaseTarget::PlayerZoomY),
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

fn reset_overlay_capture_tables(lua: &Lua, overlays: &[OverlayCompileActor]) -> Result<(), String> {
    for overlay in overlays {
        reset_actor_capture(lua, &overlay.table).map_err(|err| err.to_string())?;
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

fn capture_overlay_function_blocks(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    function: &Function,
    arg: Option<f32>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    reset_overlay_capture_tables(lua, overlays)?;
    let result = match arg {
        Some(value) => function.call::<Value>(value),
        None => function.call::<Value>(()),
    };
    let blocks = collect_overlay_capture_blocks(overlays)?;
    result.map_err(|err| err.to_string())?;
    Ok(blocks)
}

fn overlay_delta_is_empty(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.x.is_none()
        && delta.y.is_none()
        && delta.diffuse.is_none()
        && delta.visible.is_none()
        && delta.cropleft.is_none()
        && delta.cropright.is_none()
        && delta.croptop.is_none()
        && delta.cropbottom.is_none()
        && delta.zoom.is_none()
        && delta.zoom_x.is_none()
        && delta.zoom_y.is_none()
        && delta.basezoom.is_none()
        && delta.basezoom_x.is_none()
        && delta.basezoom_y.is_none()
        && delta.rot_x_deg.is_none()
        && delta.rot_y_deg.is_none()
        && delta.rot_z_deg.is_none()
        && delta.blend.is_none()
        && delta.vibrate.is_none()
        && delta.effect_magnitude.is_none()
        && delta.effect_mode.is_none()
        && delta.effect_color1.is_none()
        && delta.effect_color2.is_none()
        && delta.effect_period.is_none()
        && delta.custom_texture_rect.is_none()
        && delta.texcoord_velocity.is_none()
        && delta.size.is_none()
        && delta.stretch_rect.is_none()
}

fn merge_overlay_delta(into: &mut SongLuaOverlayStateDelta, from: &SongLuaOverlayStateDelta) {
    if from.x.is_some() {
        into.x = from.x;
    }
    if from.y.is_some() {
        into.y = from.y;
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
    if from.zoom.is_some() {
        into.zoom = from.zoom;
    }
    if from.zoom_x.is_some() {
        into.zoom_x = from.zoom_x;
    }
    if from.zoom_y.is_some() {
        into.zoom_y = from.zoom_y;
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
    if from.rot_x_deg.is_some() {
        into.rot_x_deg = from.rot_x_deg;
    }
    if from.rot_y_deg.is_some() {
        into.rot_y_deg = from.rot_y_deg;
    }
    if from.rot_z_deg.is_some() {
        into.rot_z_deg = from.rot_z_deg;
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
}

fn overlay_delta_from_blocks(
    blocks: &[SongLuaOverlayCommandBlock],
) -> Option<SongLuaOverlayStateDelta> {
    let mut delta = SongLuaOverlayStateDelta::default();
    for block in blocks {
        merge_overlay_delta(&mut delta, &block.delta);
    }
    (!overlay_delta_is_empty(&delta)).then_some(delta)
}

fn overlay_delta_intersection(
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
    copy_pair!(diffuse);
    copy_pair!(visible);
    copy_pair!(cropleft);
    copy_pair!(cropright);
    copy_pair!(croptop);
    copy_pair!(cropbottom);
    copy_pair!(zoom);
    copy_pair!(zoom_x);
    copy_pair!(zoom_y);
    copy_pair!(basezoom);
    copy_pair!(basezoom_x);
    copy_pair!(basezoom_y);
    copy_pair!(rot_x_deg);
    copy_pair!(rot_y_deg);
    copy_pair!(rot_z_deg);
    copy_pair!(blend);
    copy_pair!(vibrate);
    copy_pair!(effect_magnitude);
    copy_pair!(effect_mode);
    copy_pair!(effect_color1);
    copy_pair!(effect_color2);
    copy_pair!(effect_period);
    copy_pair!(custom_texture_rect);
    copy_pair!(texcoord_velocity);
    copy_pair!(size);
    copy_pair!(stretch_rect);
    (!overlay_delta_is_empty(&out_from)).then_some((out_from, out_to))
}

fn compile_overlay_function_action(
    lua: &Lua,
    overlays: &mut [OverlayCompileActor],
    function: &Function,
    beat: f32,
    persists: bool,
    counter: &mut usize,
    messages: &mut Vec<SongLuaMessageEvent>,
) -> Result<bool, String> {
    let captures = capture_overlay_function_blocks(lua, overlays, function, None)?;
    if captures.is_empty() {
        return Ok(false);
    }
    let message = format!("__songlua_overlay_fn_action_{}", *counter);
    *counter += 1;
    for (overlay_index, blocks) in captures {
        overlays[overlay_index]
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
    let from_blocks = capture_overlay_function_blocks(lua, overlays, function, Some(from))?;
    let to_blocks = capture_overlay_function_blocks(lua, overlays, function, Some(to))?;
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

fn read_actions(
    lua: &Lua,
    table: Option<Table>,
    overlays: &mut [OverlayCompileActor],
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
                    compile_overlay_function_action(
                        lua, overlays, &function, beat, persists, counter, messages
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

fn table_len(table: Option<Table>) -> Result<usize, String> {
    Ok(table.map(|table| table.raw_len()).unwrap_or(0))
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
fn read_string(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_str().ok()?.to_string()),
        _ => None,
    }
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
    let offset = usize::from(matches!(args.front(), Some(Value::Table(_))));
    args.get(offset + index)
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
fn is_song_lua_media_path(path: &Path) -> bool {
    is_song_lua_image_path(path) || is_song_lua_video_path(path)
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
        EffectMode, SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget,
        SongLuaOverlayBlendMode, SongLuaOverlayKind, SongLuaPlayerContext, SongLuaProxyTarget,
        SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit, compile_song_lua, file_path_string,
    };
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
            } if font_path.ends_with("_komika axis 42px.ini") && text == "3"
        ));
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
            {8, 2, 0, 10, function(x) if target then target:rotationz(x) end end, "len", ease.inOutQuad},
            {12, 1, 0, 0.15, function(x) if target then target:skewx(x) end end, "len", ease.outQuad},
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
        assert_eq!(compiled.eases.len(), 2);
        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert!(matches!(
            compiled.eases[0].target,
            SongLuaEaseTarget::PlayerRotationZ
        ));
        assert!(matches!(
            compiled.eases[1].target,
            SongLuaEaseTarget::PlayerSkewX
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
        assert_eq!(compiled.messages[0].message, "200.00");
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
        assert_eq!(compiled.messages[0].message, "true:child");
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
        assert_eq!(compiled.info.unsupported_function_eases, 1);
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
