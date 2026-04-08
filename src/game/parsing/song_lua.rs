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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompiledSongLua {
    pub entry_path: PathBuf,
    pub beat_mods: Vec<SongLuaModWindow>,
    pub time_mods: Vec<SongLuaModWindow>,
    pub eases: Vec<SongLuaEaseWindow>,
    pub messages: Vec<SongLuaMessageEvent>,
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

    let globals = lua.globals();
    let mut out = CompiledSongLua {
        entry_path,
        ..CompiledSongLua::default()
    };

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

fn run_actor_init_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(command) = actor.get::<Option<Function>>("InitCommand")? else {
        return Ok(());
    };
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
    let self_clone = actor.clone();
    for name in [
        "SetTarget",
        "visible",
        "sleep",
        "queuecommand",
        "playcommand",
        "x",
        "y",
        "xy",
        "stretchto",
        "cropright",
        "cropleft",
        "diffusealpha",
        "linear",
        "basezoom",
        "zoom",
        "rotationz",
        "skewx",
        "SetUpdateFunction",
        "addy",
        "zoomz",
        "diffuse",
    ] {
        let method_self = self_clone.clone();
        actor.set(
            name,
            lua.create_function(move |_, _args: MultiValue| Ok(method_self.clone()))?,
        )?;
    }
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
    install_actor_metatable(lua, &actor)?;
    Ok(actor)
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
            Value::Function(_) => (SongLuaEaseTarget::Function, true),
            _ => continue,
        };
        if is_function_target {
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
        SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget, SongLuaPlayerContext,
        SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit, compile_song_lua,
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
        assert!(compiled.eases.len() >= 40);
        assert!(compiled.info.unsupported_function_eases >= 2);
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
