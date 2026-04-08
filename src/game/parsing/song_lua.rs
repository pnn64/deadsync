use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};

const LUA_PLAYERS: usize = 2;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaPlayerContext {
    pub enabled: bool,
    pub difficulty: SongLuaDifficulty,
    pub speedmod: SongLuaSpeedMod,
}

impl Default for SongLuaPlayerContext {
    fn default() -> Self {
        Self {
            enabled: true,
            difficulty: SongLuaDifficulty::default_enabled(),
            speedmod: SongLuaSpeedMod::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SongLuaCompileContext {
    pub song_dir: PathBuf,
    pub main_title: String,
    pub global_offset_seconds: f32,
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
            global_offset_seconds: 0.0,
            players: [SongLuaPlayerContext::default(); LUA_PLAYERS],
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
    PlayerSkewX,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongLuaOverlayKind {
    Sprite { texture_path: PathBuf },
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
    pub zoom: f32,
    pub basezoom: f32,
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
            zoom: 1.0,
            basezoom: 1.0,
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
    pub zoom: Option<f32>,
    pub basezoom: Option<f32>,
    pub stretch_rect: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayCommandBlock {
    pub start: f32,
    pub duration: f32,
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
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
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
    pub beat_mods: Vec<SongLuaModWindow>,
    pub time_mods: Vec<SongLuaModWindow>,
    pub eases: Vec<SongLuaEaseWindow>,
    pub messages: Vec<SongLuaMessageEvent>,
    pub overlays: Vec<SongLuaOverlayActor>,
    pub info: SongLuaCompileInfo,
}

#[derive(Default)]
struct HostState {
    easing_names: HashMap<*const c_void, String>,
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
        ..CompiledSongLua::default()
    };
    out.overlays = read_overlay_actors(&lua, &root)?;

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
        let (eases, info) = read_eases(
            &lua,
            prefix_globals
                .get::<Option<Table>>("ease")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
            &host.easing_names,
        )?;
        out.eases.extend(eases);
        out.info.unsupported_function_eases += info.unsupported_function_eases;
        out.info.unsupported_perframes += table_len(
            prefix_globals
                .get::<Option<Table>>("perframes")
                .map_err(|err| err.to_string())?,
        )?;
        let (messages, fn_actions) = read_actions(
            prefix_globals
                .get::<Option<Table>>("actions")
                .map_err(|err| err.to_string())?,
        )?;
        out.messages.extend(messages);
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
    let (global_eases, global_info) = read_eases(
        &lua,
        globals
            .get::<Option<Table>>("mods_ease")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Beat,
        &host.easing_names,
    )?;
    out.eases.extend(global_eases);
    out.info.unsupported_function_eases += global_info.unsupported_function_eases;
    out.info.unsupported_perframes += table_len(
        globals
            .get::<Option<Table>>("mod_perframes")
            .map_err(|err| err.to_string())?,
    )?;
    let (global_messages, global_fn_actions) = read_actions(
        globals
            .get::<Option<Table>>("mod_actions")
            .map_err(|err| err.to_string())?,
    )?;
    out.messages.extend(global_messages);
    out.info.unsupported_function_actions += global_fn_actions;

    out.beat_mods.sort_by(mod_window_cmp);
    out.time_mods.sort_by(mod_window_cmp);
    out.eases.sort_by(ease_window_cmp);
    out.messages.sort_by(message_event_cmp);
    Ok(out)
}

fn install_host(
    lua: &Lua,
    context: &SongLuaCompileContext,
    host: &mut HostState,
) -> mlua::Result<()> {
    install_stdlib_compat(lua)?;
    install_ease_table(lua, host)?;
    install_globals(lua, context)?;
    install_def(lua)?;
    install_file_loaders(lua, context.song_dir.clone())?;
    Ok(())
}

fn install_stdlib_compat(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let table: Table = globals.get("table")?;
    table.set(
        "getn",
        lua.create_function(|_, table: Table| Ok(table.raw_len() as i64))?,
    )?;
    let string: Table = globals.get("string")?;
    if matches!(string.get::<Value>("gfind")?, Value::Nil) {
        let gmatch = string.get::<Value>("gmatch")?;
        string.set("gfind", gmatch)?;
    }
    globals.set("unpack", table.get::<Value>("unpack")?)?;
    globals.set("Trace", lua.create_function(|_, _msg: String| Ok(()))?)?;
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

fn install_globals(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("SCREEN_WIDTH", 640)?;
    globals.set("SCREEN_HEIGHT", 480)?;
    globals.set("SCREEN_CENTER_X", 320.0)?;
    globals.set("SCREEN_CENTER_Y", 240.0)?;
    globals.set("scx", 320.0)?;
    globals.set("scy", 240.0)?;
    globals.set(
        "__songlua_song_dir",
        song_dir_string(context.song_dir.as_path()),
    )?;
    globals.set("__songlua_script_dir", Value::Nil)?;

    let player_options = lua.create_table()?;
    player_options.set("ConfusionOffset", context.confusion_offset_available)?;
    player_options.set("Confusion", context.confusion_available)?;
    player_options.set("AMod", context.amod_available)?;
    player_options.set("FromString", true)?;
    globals.set("PlayerOptions", player_options)?;

    let prefsmgr = lua.create_table()?;
    let global_offset_seconds = context.global_offset_seconds;
    prefsmgr.set(
        "GetPreference",
        lua.create_function(move |_, (_self, key): (Table, String)| {
            if key.eq_ignore_ascii_case("GlobalOffsetSeconds") {
                Ok(Value::Number(global_offset_seconds as f64))
            } else {
                Ok(Value::Nil)
            }
        })?,
    )?;
    globals.set("PREFSMAN", prefsmgr)?;

    let song = create_song_table(lua, context)?;
    let players = create_player_tables(lua, context)?;
    let gamestate = lua.create_table()?;
    let song_clone = song.clone();
    gamestate.set(
        "GetCurrentSong",
        lua.create_function(move |_, _self: Table| Ok(song_clone.clone()))?,
    )?;
    let players_enabled = context.players;
    gamestate.set(
        "IsPlayerEnabled",
        lua.create_function(move |_, (_self, player): (Table, Value)| {
            let Some(player) = player_index_from_value(&player) else {
                return Ok(false);
            };
            Ok(players_enabled[player].enabled)
        })?,
    )?;
    let player_states = players.player_states.clone();
    gamestate.set(
        "GetPlayerState",
        lua.create_function(move |_, (_self, player): (Table, Value)| {
            let Some(player) = player_index_from_value(&player) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(player_states[player].clone()))
        })?,
    )?;
    let current_steps = players.steps.clone();
    gamestate.set(
        "GetCurrentSteps",
        lua.create_function(move |_, (_self, player): (Table, Value)| {
            let Some(player) = player_index_from_value(&player) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(current_steps[player].clone()))
        })?,
    )?;
    gamestate.set(
        "GetSongBeat",
        lua.create_function(|_, _self: Table| Ok(0.0_f32))?,
    )?;
    gamestate.set(
        "GetCurMusicSeconds",
        lua.create_function(|_, _self: Table| Ok(0.0_f32))?,
    )?;
    let song_position = create_song_position_table(lua)?;
    gamestate.set(
        "GetSongPosition",
        lua.create_function(move |_, _self: Table| Ok(song_position.clone()))?,
    )?;
    globals.set("GAMESTATE", gamestate)?;

    let screenman = lua.create_table()?;
    let top_screen = create_dummy_actor(lua, "TopScreen")?;
    screenman.set(
        "GetTopScreen",
        lua.create_function(move |_, _self: Table| Ok(top_screen.clone()))?,
    )?;
    screenman.set(
        "SystemMessage",
        lua.create_function(|_, (_self, _msg): (Table, String)| Ok(()))?,
    )?;
    globals.set("SCREENMAN", screenman)?;

    let messageman = lua.create_table()?;
    messageman.set(
        "Broadcast",
        lua.create_function(|_, (_self, _msg): (Table, String)| Ok(()))?,
    )?;
    globals.set("MESSAGEMAN", messageman)?;
    Ok(())
}

struct PlayerLuaTables {
    player_states: [Table; LUA_PLAYERS],
    steps: [Table; LUA_PLAYERS],
}

fn create_player_tables(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<PlayerLuaTables> {
    let player_states = [
        create_player_state_table(lua, context.players[0])?,
        create_player_state_table(lua, context.players[1])?,
    ];
    let steps = [
        create_steps_table(lua, context.players[0].difficulty)?,
        create_steps_table(lua, context.players[1].difficulty)?,
    ];
    Ok(PlayerLuaTables {
        player_states,
        steps,
    })
}

fn create_player_state_table(lua: &Lua, player: SongLuaPlayerContext) -> mlua::Result<Table> {
    let options = create_player_options_table(lua, player)?;
    let table = lua.create_table()?;
    table.set(
        "GetPlayerOptions",
        lua.create_function(move |_, (_self, _level): (Table, Value)| Ok(options.clone()))?,
    )?;
    Ok(table)
}

fn create_steps_table(lua: &Lua, difficulty: SongLuaDifficulty) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let difficulty = difficulty.sm_name().to_string();
    table.set(
        "GetDifficulty",
        lua.create_function(move |lua, _self: Table| {
            Ok(Value::String(lua.create_string(&difficulty)?))
        })?,
    )?;
    Ok(table)
}

fn create_player_options_table(lua: &Lua, player: SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    install_speedmod_method(lua, &table, "CMod", player.speedmod, SongLuaSpeedMod::C)?;
    install_speedmod_method(lua, &table, "MMod", player.speedmod, SongLuaSpeedMod::M)?;
    install_speedmod_method(lua, &table, "AMod", player.speedmod, SongLuaSpeedMod::A)?;
    install_speedmod_method(lua, &table, "XMod", player.speedmod, SongLuaSpeedMod::X)?;
    let mt = lua.create_table()?;
    let fallback_owner = table.clone();
    mt.set(
        "__index",
        lua.create_function(move |lua, (_self, key): (Table, Value)| {
            let Some(name) = read_string(key) else {
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
        lua.create_function(move |_, _self: Table| Ok(speedmod_value(speedmod, ctor)))?,
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
        lua.create_function(move |lua, _self: Table| {
            Ok(Value::String(lua.create_string(&song_dir)?))
        })?,
    )?;
    let title = context.main_title.clone();
    table.set(
        "GetMainTitle",
        lua.create_function(move |lua, _self: Table| {
            Ok(Value::String(lua.create_string(&title)?))
        })?,
    )?;
    let timing = create_timing_table(lua)?;
    table.set(
        "GetTimingData",
        lua.create_function(move |_, _self: Table| Ok(timing.clone()))?,
    )?;
    Ok(table)
}

fn create_timing_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetElapsedTimeFromBeat",
        lua.create_function(|_, (_self, beat): (Table, f32)| Ok(beat))?,
    )?;
    table.set(
        "GetBeatFromElapsedTime",
        lua.create_function(|_, (_self, time): (Table, f32)| Ok(time))?,
    )?;
    Ok(table)
}

fn create_song_position_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetSongBeat",
        lua.create_function(|_, _self: Table| Ok(0.0_f32))?,
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
        ("Quad", "Quad"),
        ("ActorProxy", "ActorProxy"),
        ("ActorFrameTexture", "ActorFrameTexture"),
    ] {
        def.set(name, make_actor_ctor(lua, actor_type)?)?;
    }
    globals.set("Def", def)?;
    Ok(())
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
        install_actor_methods(lua, &table)?;
        install_actor_metatable(lua, &table)?;
        Ok(table)
    })
}

fn install_file_loaders(lua: &Lua, song_dir: PathBuf) -> mlua::Result<()> {
    let globals = lua.globals();
    let song_dir_for_loadfile = song_dir.clone();
    globals.set(
        "loadfile",
        lua.create_function(move |lua, path: String| {
            create_loader_function(lua, &song_dir_for_loadfile, &path)
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
                let loader = create_loader_function(lua, &song_dir, &path)?;
                match loader.call::<Value>(())? {
                    Value::Table(table) => Ok(table),
                    _ => create_dummy_actor(lua, "LoadActor"),
                }
            }
            _ => create_dummy_actor(lua, "LoadActor"),
        })?,
    )?;
    Ok(())
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
    let chunk = lua.load(&source).set_name(path.to_string_lossy().as_ref());
    let inner = chunk.into_function()?;
    let script_dir = path.parent().unwrap_or(song_dir).to_path_buf();
    lua.create_function(move |lua, args: MultiValue| {
        call_with_script_dir(lua, &script_dir, || inner.call::<Value>(args.clone()))
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
    let actor_type = actor.get::<Option<String>>("__songlua_actor_type")?;
    Ok(!matches!(
        actor_type.as_deref(),
        Some("Sprite") | Some("Quad")
    ))
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
    let result = call_actor_function(lua, actor, command);
    active.set(name, Value::Nil)?;
    result
}

fn actor_active_commands(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(active) = actor.get::<Option<Table>>("__songlua_active_commands")? {
        return Ok(active);
    }
    let active = lua.create_table()?;
    actor.set("__songlua_active_commands", active.clone())?;
    Ok(active)
}

fn read_overlay_actors(lua: &Lua, root: &Value) -> Result<Vec<SongLuaOverlayActor>, String> {
    let Value::Table(root) = root else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    read_overlay_actors_from_table(lua, root, &mut out)?;
    Ok(out)
}

fn read_overlay_actors_from_table(
    lua: &Lua,
    actor: &Table,
    out: &mut Vec<SongLuaOverlayActor>,
) -> Result<(), String> {
    if let Some(overlay) = read_overlay_actor(lua, actor)? {
        out.push(overlay);
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        read_overlay_actors_from_table(lua, &child, out)?;
    }
    Ok(())
}

fn read_overlay_actor(lua: &Lua, actor: &Table) -> Result<Option<SongLuaOverlayActor>, String> {
    let Some(actor_type) = actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let kind = if actor_type.eq_ignore_ascii_case("Sprite") {
        let Some(texture_path) = actor
            .get::<Option<String>>("Texture")
            .map_err(|err| err.to_string())?
            .and_then(|texture| resolve_actor_asset_path(actor, &texture).ok())
        else {
            return Ok(None);
        };
        SongLuaOverlayKind::Sprite { texture_path }
    } else if actor_type.eq_ignore_ascii_case("Quad") {
        SongLuaOverlayKind::Quad
    } else {
        return Ok(None);
    };
    let on_command = capture_actor_command(lua, actor, "OnCommand")?;
    let initial_state =
        overlay_state_after_blocks(SongLuaOverlayState::default(), &on_command, 0.0);
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
    Ok(Some(SongLuaOverlayActor {
        kind,
        name,
        initial_state,
        message_commands,
    }))
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
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

fn capture_block_set_f32(lua: &Lua, actor: &Table, key: &str, value: f32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    Ok(())
}

fn capture_block_set_bool(lua: &Lua, actor: &Table, key: &str, value: bool) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    Ok(())
}

fn capture_block_set_color(lua: &Lua, actor: &Table, color: [f32; 4]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, color[0])?;
    value.raw_set(2, color[1])?;
    value.raw_set(3, color[2])?;
    value.raw_set(4, color[3])?;
    block.set("diffuse", value)?;
    block.set("__songlua_has_changes", true)?;
    Ok(())
}

fn capture_block_set_stretch(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, rect[0])?;
    value.raw_set(2, rect[1])?;
    value.raw_set(3, rect[2])?;
    value.raw_set(4, rect[3])?;
    block.set("stretch_rect", value)?;
    block.set("__songlua_has_changes", true)?;
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
        out.push(SongLuaOverlayCommandBlock {
            start,
            duration,
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
                zoom: block
                    .get::<Option<f32>>("zoom")
                    .map_err(|err| err.to_string())?,
                basezoom: block
                    .get::<Option<f32>>("basezoom")
                    .map_err(|err| err.to_string())?,
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
    if let Some(value) = delta.zoom {
        state.zoom = value;
    }
    if let Some(value) = delta.basezoom {
        state.basezoom = value;
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
    if delta.zoom.is_some() {
        from.zoom = (to.zoom - from.zoom).mul_add(t, from.zoom);
    }
    if delta.basezoom.is_some() {
        from.basezoom = (to.basezoom - from.basezoom).mul_add(t, from.basezoom);
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
    from
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

fn create_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
    let actor = lua.create_table()?;
    actor.set("__songlua_actor_type", actor_type)?;
    install_actor_methods(lua, &actor)?;
    install_actor_metatable(lua, &actor)?;
    Ok(actor)
}

fn install_actor_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "SetTarget",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor.clone())
        })?,
    )?;
    actor.set(
        "visible",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).map(truthy) {
                    capture_block_set_bool(lua, &actor, "visible", value)?;
                }
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
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                flush_actor_capture(&actor)?;
                actor.set(
                    "__songlua_capture_duration",
                    args.get(1)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0)
                        .max(0.0),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["accelerate", "decelerate", "smooth"] {
        actor.set(name, make_actor_tween_method(lua, actor)?)?;
    }
    actor.set(
        "queuecommand",
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
                        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
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
    for name in ["rotationz", "skewx", "addy", "zoomz"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                let method_name = name.to_string();
                move |lua, _args: MultiValue| {
                    record_probe_method_call(lua, &method_name)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    for name in [
        "blend",
        "clearzbuffer",
        "cropbottom",
        "croptop",
        "customtexturerect",
        "diffuseshift",
        "effectclock",
        "effectcolor1",
        "effectcolor2",
        "effectmagnitude",
        "effectperiod",
        "fardistz",
        "fov",
        "rotationx",
        "rotationy",
        "spin",
        "texcoordvelocity",
        "vibrate",
        "wag",
        "z",
        "zoomto",
        "zoomx",
        "zoomy",
    ] {
        actor.set(name, make_actor_chain_method(lua, actor)?)?;
    }
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
    let child = actor.clone();
    actor.set(
        "GetChild",
        lua.create_function(move |_, (_self, _name): (Table, String)| Ok(child.clone()))?,
    )?;
    let wrapper = actor.clone();
    actor.set(
        "GetWrapperState",
        lua.create_function(move |_, (_self, _index): (Table, i64)| Ok(wrapper.clone()))?,
    )?;
    actor.set(
        "GetNumWrapperStates",
        lua.create_function(|_, _self: Table| Ok(0_i64))?,
    )?;
    let add_wrapper = actor.clone();
    actor.set(
        "AddWrapperState",
        lua.create_function(move |_, _self: Table| Ok(add_wrapper.clone()))?,
    )?;
    actor.set(
        "GetChildren",
        lua.create_function(|lua, _self: Table| lua.create_table())?,
    )?;
    actor.set(
        "GetNumChildren",
        lua.create_function(|_, _self: Table| Ok(0_i64))?,
    )?;
    actor.set("GetX", lua.create_function(|_, _self: Table| Ok(0.0_f32))?)?;
    actor.set("GetY", lua.create_function(|_, _self: Table| Ok(0.0_f32))?)?;
    actor.set(
        "GetZoom",
        lua.create_function(|_, _self: Table| Ok(1.0_f32))?,
    )?;
    Ok(())
}

fn install_actor_metatable(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let mt = lua.create_table()?;
    let actor_clone = actor.clone();
    mt.set(
        "__concat",
        lua.create_function(move |_, (_lhs, _rhs): (Value, Value)| Ok(actor_clone.clone()))?,
    )?;
    let _ = actor.set_metatable(Some(mt));
    Ok(())
}

fn make_actor_chain_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |_, _args: MultiValue| Ok(actor.clone()))
}

fn make_actor_tween_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
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
        Ok(actor.clone())
    })
}

fn read_color_args(args: &MultiValue) -> Option<[f32; 4]> {
    if args.len() < 5 {
        return None;
    }
    Some([
        args.get(1).cloned().and_then(read_f32)?,
        args.get(2).cloned().and_then(read_f32)?,
        args.get(3).cloned().and_then(read_f32)?,
        args.get(4).cloned().and_then(read_f32)?,
    ])
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
) -> Result<(Vec<SongLuaEaseWindow>, SongLuaCompileInfo), String> {
    let Some(table) = table else {
        return Ok((Vec::new(), SongLuaCompileInfo::default()));
    };
    let mut out = Vec::new();
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
        let target_value = entry.raw_get::<Value>(5).map_err(|err| err.to_string())?;
        let (target, is_function_target) = match target_value {
            Value::String(text) => (
                SongLuaEaseTarget::Mod(text.to_str().map_err(|err| err.to_string())?.to_string()),
                false,
            ),
            Value::Function(function) => (
                probe_function_ease_target(lua, &function)
                    .map_err(|err| err.to_string())?
                    .unwrap_or(SongLuaEaseTarget::Function),
                true,
            ),
            _ => continue,
        };
        if is_function_target && matches!(target, SongLuaEaseTarget::Function) {
            info.unsupported_function_eases += 1;
        }

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

        out.push(SongLuaEaseWindow {
            unit,
            start,
            limit,
            span_mode,
            from,
            to,
            target,
            easing: read_easing_name(easing_value, easing_names),
            player: read_player(player_value),
            sustain: read_f32(sustain_value),
            opt1: read_f32(opt1_value),
            opt2: read_f32(opt2_value),
        });
    }
    Ok((out, info))
}

fn record_probe_method_call(lua: &Lua, method_name: &str) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(calls) = globals.get::<Option<Table>>("__songlua_probe_methods")? else {
        return Ok(());
    };
    calls.raw_set(calls.raw_len() + 1, method_name)?;
    Ok(())
}

fn probe_function_ease_target(
    lua: &Lua,
    function: &Function,
) -> mlua::Result<Option<SongLuaEaseTarget>> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("__songlua_probe_methods")?;
    let calls = lua.create_table()?;
    globals.set("__songlua_probe_methods", calls.clone())?;
    let result = function.call::<Value>(1.0_f32);
    let classify = classify_function_ease_probe(&calls);
    globals.set("__songlua_probe_methods", previous)?;
    match result {
        Ok(_) => classify,
        Err(_) => Ok(None),
    }
}

fn classify_function_ease_probe(calls: &Table) -> mlua::Result<Option<SongLuaEaseTarget>> {
    let mut saw_rotation_z = false;
    let mut saw_skew_x = false;
    for value in calls.sequence_values::<String>() {
        match value?.as_str() {
            "rotationz" => saw_rotation_z = true,
            "skewx" => saw_skew_x = true,
            _ => return Ok(None),
        }
    }
    Ok(match (saw_rotation_z, saw_skew_x) {
        (true, false) => Some(SongLuaEaseTarget::PlayerRotationZ),
        (false, true) => Some(SongLuaEaseTarget::PlayerSkewX),
        _ => None,
    })
}

fn read_actions(table: Option<Table>) -> Result<(Vec<SongLuaMessageEvent>, usize), String> {
    let Some(table) = table else {
        return Ok((Vec::new(), 0));
    };
    let mut out = Vec::new();
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
            Value::String(text) => out.push(SongLuaMessageEvent {
                beat,
                message: text.to_str().map_err(|err| err.to_string())?.to_string(),
                persists,
            }),
            Value::Function(_) => unsupported_function_actions += 1,
            _ => {}
        }
    }
    Ok((out, unsupported_function_actions))
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
fn song_dir_string(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if !text.ends_with('/') {
        text.push('/');
    }
    text
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
        SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget, SongLuaOverlayKind,
        SongLuaPlayerContext, SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit, compile_song_lua,
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
            },
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::C(516.0),
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
            },
            SongLuaPlayerContext {
                enabled: false,
                difficulty: SongLuaDifficulty::Easy,
                speedmod: SongLuaSpeedMod::X(1.0),
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
}
