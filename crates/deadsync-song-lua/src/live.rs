use mlua::{Function, Lua, MultiValue, Table, Value};
use std::fmt;

use crate::{SongLuaCompileContext, SongLuaOverlayState, SongLuaTrackedActorTarget, LUA_PLAYERS};

pub(crate) const TAP_SCORES: [&str; 11] = [
    "None", "HitMine", "AvoidMine", "CheckpointMiss", "Miss", "W5",
    "W4", "W3", "W2", "W1", "CheckpointHit",
];

pub(crate) struct LiveMode(pub u64);

#[derive(Default)]
struct LiveChartAttacks {
    windows: std::sync::Arc<[deadsync_gameplay::ChartAttackWindow]>,
    active: Vec<usize>,
    pending: Vec<usize>,
}

impl LiveChartAttacks {
    fn update(&mut self, lua: &Lua, player: usize, master: bool,
              windows: &std::sync::Arc<[deadsync_gameplay::ChartAttackWindow]>, seconds: f32)
              -> mlua::Result<()> {
        let replaced = !std::sync::Arc::ptr_eq(&self.windows, windows);
        if replaced {
            self.windows = windows.clone();
            self.pending.reserve(windows.len());
            self.active.reserve(windows.len());
        }
        self.pending.clear();
        for (index, attack) in windows.iter().enumerate() {
            // PlayerState::Update uses strict endpoints, including expiry.
            if attack.start_second < seconds && seconds < attack.start_second + attack.len_seconds {
                self.pending.push(index);
            }
        }
        if self.active != self.pending || (replaced && !self.active.is_empty()) {
            crate::song_tables::rebuild_active_attack_options(lua, player, master,
                self.pending.iter().map(|&index| windows[index].mods.as_str()))?;
            std::mem::swap(&mut self.active, &mut self.pending);
        }
        Ok(())
    }
}

struct LiveForeground {
    actor: Table,
    start_beat: f32,
    stop_beat: f32,
    started: bool,
    finished: bool,
}

struct LiveBackground {
    actor: Table,
}

#[derive(Default)]
struct LiveBackgroundLayer {
    changes: Vec<(usize, deadsync_chart::song::SongBackgroundLuaActivation)>,
    change_index: Option<usize>,
    current: Option<usize>,
    fading: Option<usize>,
}

pub(crate) fn active(lua: &Lua) -> bool {
    lua.app_data_ref::<LiveMode>().is_some()
}

// The song observer starts after screen/theme construction, including when
// a retained VM replaces its screen. Keep the actual native callbacks running.
fn query_trace_ready(lua: &Lua) -> bool {
    let Some(scene) = lua.app_data_ref::<LiveMode>().map(|mode| mode.0) else { return false; };
    lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.scene == scene)
}

// Native SongLuaTrace.lua wraps authored callbacks only. Reserved theme chunks
// run normally; nested calls remain visible under an existing song command.
pub(crate) fn observes_song_callback(callback: &Function) -> bool {
    !callback.info().source.is_some_and(|source| source.starts_with("@theme:"))
}

// Trace logging observes the live VM without replacing callbacks or query results.
pub(crate) fn trace_command(lua: &Lua, command: &Function, name: &str, phase: &str) {
    if !query_trace_ready(lua) || !observes_song_callback(command) { return; }
    callback_phase(lua, phase == "begin");
    if !log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) { return; }
    let info = command.info();
    let Some(source) = info.source else { return; };
    if let Ok((beat, seconds)) = crate::compile_song_runtime_values(lua) {
        let scene = lua.app_data_ref::<LiveMode>().map_or(0, |mode| mode.0);
        emit_trace(serde_json::json!({
            "scene":scene,"kind":"command","operation":format!("command.{phase}"),
            "command":name,"source":source,"line":info.line_defined,"beat":beat,"seconds":seconds
        }));
    }
}

pub(crate) fn callback_phase(lua: &Lua, entering: bool) {
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() {
        if entering { trace.commands += 1; }
        else { trace.commands = trace.commands.saturating_sub(1); }
    }
}

pub(crate) fn observes_source_call(lua: &Lua) -> bool {
    if lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.commands != 0) {
        return true;
    }
    let source = (1..32).find_map(|level| lua.inspect_stack(level, |debug| {
        let info = debug.source();
        (info.what != "C").then(|| info.source.map(|source| source.into_owned()))
    }).flatten());
    !source.flatten().is_some_and(|source| source.starts_with("@theme:"))
}

pub(crate) fn trace_stage_query(lua: &Lua, operation: &str, points: i64) {
    trace_method(lua, "query", operation, || serde_json::json!({"args":[],"results":[points]}));
}

pub(crate) fn trace_actor_call(lua: &Lua, actor: &Table, operation: &str, arg: Option<&Value>) {
    trace_method(lua, "call", operation, || {
        let arg = match arg {
            Some(Value::Integer(value)) => serde_json::json!(value),
            Some(Value::Number(value)) if value.is_finite() => serde_json::json!(value),
            Some(Value::Number(value)) => serde_json::json!({"type":"number", "value":if value.is_nan() {
                "nan"
            } else if value.is_sign_negative() { "-infinity" } else { "infinity" }}),
            Some(Value::String(value)) => serde_json::json!(value.to_string_lossy()),
            _ => serde_json::Value::Null,
        };
        let left = crate::lua_util::actor_tween_time_left(lua, actor);
        match left {
            Ok(left) => serde_json::json!({"args":[arg], "tween_time_left":if left.is_finite() {
                serde_json::json!(left)
            } else { serde_json::json!({"type":"number", "value":if left.is_nan() {
                "nan"
            } else if left.is_sign_negative() { "-infinity" } else { "infinity" }}) }}),
            Err(error) => serde_json::json!({"kind":"trace-error", "error":error.to_string()}),
        }
    });
}

fn trace_method(lua: &Lua, kind: &str, operation: &str, details: impl FnOnce() -> serde_json::Value) {
    if !query_trace_ready(lua) || !log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) { return; }
    if lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.depth != 0) { return; }
    // Actor class dispatch adds a C adapter frame before the authored Lua
    // caller. Inspect the first Lua frame instead of attributing it to C.
    let source = (1..32).find_map(|level| lua.inspect_stack(level, |debug| {
        let info = debug.source();
        (info.what != "C").then(|| (info.source.map(|source| source.into_owned()), info.line_defined))
    }).flatten());
    if let (Some((Some(source), line)), Ok((beat, seconds))) =
        (source, crate::compile_song_runtime_values(lua))
    {
        let in_song_command = lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.commands != 0);
        if source.starts_with("@theme:") && !in_song_command { return; }
        let scene = lua.app_data_ref::<LiveMode>().map_or(0, |mode| mode.0);
        let phase = if in_song_command {
            "callback"
        } else { "construction" };
        let mut event = serde_json::json!({"scene":scene,"kind":kind,"operation":operation,
            "source":source,"line":line,"phase":phase,"beat":beat,"seconds":seconds});
        if let serde_json::Value::Object(details) = details() {
            event.as_object_mut().expect("trace event is an object").extend(details);
        }
        emit_trace(event);
    }
}

fn emit_trace(event: serde_json::Value) {
    #[cfg(feature = "test-support")]
    crate::trace_capture::emit(event);
    #[cfg(not(feature = "test-support"))]
    log::trace!(target: "deadsync_song_lua::trace", "{event}");
}

// Diagnostic-only registry, owned by the Lua VM. It retains identities and
// original host bindings across screen reloads; no tracing work runs by default.
#[derive(Default)]
struct QueryTrace {
    scene: u64,
    objects: Vec<(Table, serde_json::Value)>,
    hooks: Vec<(Table, &'static str)>,
    depth: usize,
    commands: usize,
    dynamic_commands: bool,
}

#[cfg(feature = "test-support")]
pub(crate) fn set_dynamic_command_observer(lua: &Lua, enabled: bool) {
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() {
        trace.dynamic_commands = enabled;
    }
}

pub(crate) fn observes_dynamic_commands(lua: &Lua) -> bool {
    lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.dynamic_commands)
}

fn query_value(lua: &Lua, value: &Value, depth: usize) -> mlua::Result<serde_json::Value> {
    use serde_json::json;
    Ok(match value {
        Value::Nil => json!({"type":"nil"}),
        Value::Boolean(value) => json!(value),
        Value::Integer(value) => json!(value),
        Value::Number(value) if value.is_finite() => json!(value),
        Value::Number(value) => json!({"type":"number","value":if value.is_nan() {"nan"}
            else if value.is_sign_negative() {"-infinity"} else {"infinity"}}),
        Value::String(value) => json!(value.to_str()?.to_string()),
        Value::Function(value) => {
            let info = value.info();
            json!({"type":"function","source":info.source.unwrap_or_default(),
                "line":info.line_defined.map_or(-1, |line| line as i64),
                "last_line":info.last_line_defined.map_or(-1, |line| line as i64)})
        }
        Value::Table(table) => {
            if table.raw_get::<Option<String>>("__songlua_actor_type")?.is_some() {
                let name = query_actor_name(table)?;
                let mut path = Vec::new();
                let mut current = Some(table.clone());
                let mut seen = rustc_hash::FxHashSet::default();
                for _ in 0..32 {
                    let Some(actor) = current.take() else { break; };
                    if !seen.insert(actor.to_pointer()) {
                        return Err(mlua::Error::runtime("cycle in actor query ancestry"));
                    }
                    let name = query_actor_name(&actor)?;
                    if !name.is_empty() { path.push(name); }
                    current = actor.raw_get::<Option<Table>>("__songlua_parent")?;
                }
                if current.is_some() {
                    return Err(mlua::Error::runtime("actor query ancestry exceeds capture depth"));
                }
                path.reverse();
                return Ok(json!({"actor":format!("{:p}", table.to_pointer()),"name":name,"path":path.join("/")}));
            }
            if let Some(kind) = table.raw_get::<Option<String>>("__songlua_native_type")? {
                return Ok(json!({"type":kind}));
            }
            if let Some(value) = lua.app_data_ref::<QueryTrace>().and_then(|trace|
                trace.objects.iter().find(|(known, _)| known.to_pointer() == table.to_pointer())
                    .map(|(_, value)| value.clone())) { return Ok(value); }
            if depth >= 5 { return Ok(json!({"type":"table","truncated":"depth"})); }
            let values = (1..=table.raw_len()).map(|index|
                query_value(lua, &table.raw_get::<Value>(index)?, depth + 1))
                .collect::<mlua::Result<Vec<_>>>()?;
            json!(values)
        }
        _ => json!({"type":value.type_name()}),
    })
}

fn query_actor_name(actor: &Table) -> mlua::Result<String> {
    crate::lua_util::actor_name(actor)
}

fn query_details(lua: &Lua, args: &MultiValue, result: &mlua::Result<MultiValue>) -> mlua::Result<serde_json::Value> {
    let args = args.iter().skip(1).map(|arg| query_value(lua, arg, 0))
        .collect::<mlua::Result<Vec<_>>>()?;
    let values = result.as_ref().map_err(Clone::clone)?;
    let results = values.iter().map(|value| query_value(lua, value, 0))
        .collect::<mlua::Result<Vec<_>>>()?;
    Ok(serde_json::json!({"args":args,"results":results}))
}

pub(crate) fn trace_bound_method(lua: &Lua, name: &'static str, args: &MultiValue, result: &mlua::Result<MultiValue>) {
    let class = match name {
        "GetChild" | "GetNumChildren" | "SetUpdateFunction" => "ActorFrame",
        "SetTarget" => "ActorProxy",
        // AMV defines its own getter on Actor, outside the existing observed
        // texture method set. Its call must not insert a Sprite observation.
        "GetTexture" if args.front().and_then(Value::as_table).is_some_and(|actor|
            actor.raw_get::<Option<String>>("__songlua_actor_type").ok().flatten().as_deref()
                == Some("ActorMultiVertex")) => return,
        "GetTexture" if args.front().and_then(Value::as_table).is_some_and(|actor|
            actor.raw_get::<Option<String>>("__songlua_actor_type").ok().flatten().as_deref()
                == Some("ActorFrameTexture")) => "ActorFrameTexture",
        "GetTexture" | "loop" | "rate" => "Sprite",
        _ => "Actor",
    };
    let kind = if name.starts_with("Get") || name == "getaux" { "query" } else { "call" };
    trace_method(lua, kind, &format!("{class}.{name}"), || {
        let details = || -> mlua::Result<serde_json::Value> {
            let mut details = query_details(lua, args, result)?;
            if kind == "call" && let Some(Value::Table(actor)) = args.front() {
                let left = crate::lua_util::actor_tween_time_left(lua, actor)?;
                details["tween_time_left"] = query_value(lua, &Value::Number(f64::from(left)), 0)?;
            }
            Ok(details)
        };
        details().unwrap_or_else(|error| serde_json::json!({"kind":"trace-error","error":error.to_string()}))
    });
}

fn hook_query(lua: &Lua, table: &Table, class: &'static str, name: &'static str) -> mlua::Result<()> {
    // Hook the native class binding itself. An instance-local recorder wrapper
    // would shadow a later authored GameState method override.
    let class_methods = if class == "GameState" {
        table.raw_get::<Option<Table>>("__songlua_class_methods")?
    } else { None };
    let table = class_methods.as_ref().unwrap_or(table);
    if lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.hooks.iter().any(|(known, method)|
        known.to_pointer() == table.to_pointer() && *method == name)) { return Ok(()); }
    let original: Function = table.get(name)?;
    table.raw_set(name, lua.create_function(move |lua, args: MultiValue| {
        let observed = lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.depth == 0);
        query_phase(lua, true);
        let result = original.call::<MultiValue>(args.clone());
        query_phase(lua, false);
        if class == "PlayerState" && name == "GetPlayerOptions"
            && let (Some(Value::Table(state)), Some(Value::Table(options)), Some(level)) =
                (args.front(), result.as_ref().ok().and_then(|values| values.front()), args.get(1))
        {
            // The native capture labels the existing option object with the
            // raw getter argument, including numeric ModsLevel aliases.
            let level = lua.coerce_string(level.clone())?
                .map(|value| value.to_string_lossy()).unwrap_or_else(|| "nil".into());
            let state_path = lua.app_data_ref::<QueryTrace>().and_then(|trace|
                trace.objects.iter().find(|(known, _)| known.to_pointer() == state.to_pointer())
                    .and_then(|(_, value)| value["path"].as_str()).map(str::to_owned))
                .unwrap_or_else(|| "player-state".into());
            if let Some(mut trace) = lua.app_data_mut::<QueryTrace>()
                && let Some((_, value)) = trace.objects.iter_mut()
                    .find(|(known, _)| known.to_pointer() == options.to_pointer())
            {
                value["path"] = serde_json::Value::String(format!("{state_path}/options:{level}"));
            }
        }
        if observed {
            let kind = if name.starts_with("Get") || name.starts_with("Is") || name.starts_with("Supports")
                || matches!(class, "PlayerOptions" | "SongOptions") && args.len() == 1 { "query" }
                else if matches!(class, "PlayerOptions" | "SongOptions")
                    || class == "PlayerState" && name == "SetPlayerOptions" { "modifier" } else { "call" };
            trace_method(lua, kind, &format!("{class}.{name}"), || {
                query_details(lua, &args, &result)
                    .unwrap_or_else(|error| serde_json::json!({"kind":"trace-error","error":error.to_string()}))
            });
        }
        result
    })?)?;
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() { trace.hooks.push((table.clone(), name)); }
    Ok(())
}

pub(crate) fn query_phase(lua: &Lua, entering: bool) {
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() {
        if entering { trace.depth += 1; }
        else { trace.depth = trace.depth.saturating_sub(1); }
    }
}

pub(crate) fn install_query_trace(lua: &Lua) -> mlua::Result<()> {
    if !active(lua) { return Ok(()); }
    if lua.app_data_ref::<QueryTrace>().is_none() { lua.set_app_data(QueryTrace::default()); }
    let scene = lua.app_data_ref::<LiveMode>().map_or(0, |mode| mode.0);
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() { trace.scene = scene; }
    // Source message provenance needs callback scope even without diagnostic
    // logging. Only the method hooks and event payloads depend on that logger.
    if !log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) { return Ok(()); }
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() { trace.depth += 1; }
    let install = || -> mlua::Result<()> {
        use serde_json::json;
        let globals = lua.globals();
        let gamestate: Table = globals.raw_get("GAMESTATE")?;
        let song: Table = gamestate.get::<Function>("GetCurrentSong")?.call(gamestate.clone())?;
        let song_dir: String = song.get::<Function>("GetSongDir")?.call(song.clone())?;
        let steps: Table = song.raw_get("__songlua_steps")?;
        let statsman: Table = globals.raw_get("STATSMAN")?;
        let stage: Table = statsman.get::<Function>("GetCurStageStats")?.call(statsman.clone())?;
        let players: Table = stage.raw_get("__songlua_player_stats")?;
        let screenman: Table = globals.raw_get("SCREENMAN")?;
        let mut objects = vec![(song.clone(), json!({"type":"Song","song_dir":song_dir})),
            (stage.clone(), json!({"type":"StageStats","path":"stage:current"}))];
        let style: Table = gamestate.get::<Function>("GetCurrentStyle")?.call(gamestate.clone())?;
        let style_name: String = style.get::<Function>("GetName")?.call(style.clone())?;
        objects.push((style.clone(), json!({"type":"Style","name":style_name})));
        hook_query(lua, &style, "Style", "GetName")?;
        for player in 0..LUA_PLAYERS {
            let state: Table = gamestate.get::<Function>("GetPlayerState")?.call((gamestate.clone(), player))?;
            let player_name = crate::player_number_name(player);
            objects.push((state.clone(), json!({"type":"PlayerState","path":format!("player-state:{player_name}")})));
            for level in ["ModsLevel_Preferred", "ModsLevel_Stage", "ModsLevel_Song", "ModsLevel_Current"] {
                let options: Table = state.get::<Function>("GetPlayerOptions")?.call((state.clone(), level))?;
                objects.push((options.clone(), json!({"type":"PlayerOptions","path":format!("player-state:{player_name}/options:{level}")})));
                for name in ["Reverse", "Boost", "ConfusionOffset", "Tiny1", "Tiny2", "Tiny3", "Tiny4"] {
                    hook_query(lua, &options, "PlayerOptions", name)?;
                }
            }
            for name in ["GetPlayerOptions", "GetPlayerOptionsString", "SetPlayerOptions"] {
                hook_query(lua, &state, "PlayerState", name)?;
            }
        }
        for level in ["ModsLevel_Preferred", "ModsLevel_Stage", "ModsLevel_Song", "ModsLevel_Current"] {
            let options: Table = gamestate.get::<Function>("GetSongOptionsObject")?.call((gamestate.clone(), level))?;
            objects.push((options.clone(), json!({"type":"SongOptions"})));
            hook_query(lua, &options, "SongOptions", "MusicRate")?;
        }
        for index in 1..=steps.raw_len() {
            let step: Table = steps.raw_get(index)?;
            let string = |method| step.get::<Function>(method)?.call::<String>(step.clone());
            let meter: i64 = step.get::<Function>("GetMeter")?.call(step.clone())?;
            objects.push((step.clone(), json!({"type":"Steps","song_dir":song_dir,"index":index,
                "steps_type":string("GetStepsType")?,"difficulty":string("GetDifficulty")?,
                "description":string("GetDescription")?,"meter":meter})));
            hook_query(lua, &step, "Steps", "GetDifficulty")?;
            hook_query(lua, &step, "Steps", "GetStepsType")?;
        }
        for index in 1..=players.raw_len() {
            objects.push((players.raw_get::<Table>(index)?,
                json!({"type":"PlayerStageStats","path":format!("stage:current/player:P{index}")})));
        }
        if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() { trace.objects = objects; }
        for name in ["GetCurrentSong", "GetCurrentSteps", "GetCurrentStyle", "GetPlayerState", "GetSongBeat", "IsPlayerEnabled", "SetCurrentSteps", "GetSongOptionsObject"] {
            hook_query(lua, &gamestate, "GameState", name)?;
        }
        hook_query(lua, &song, "Song", "GetStepsByStepsType")?;
        hook_query(lua, &song, "Song", "GetSongDir")?;
        hook_query(lua, &globals.raw_get::<Table>("DISPLAY")?, "RageDisplay", "SupportsRenderToTexture")?;
        hook_query(lua, &globals.raw_get::<Table>("PREFSMAN")?, "PrefsManager", "GetPreference")?;
        hook_query(lua, &statsman, "StatsManager", "GetCurStageStats")?;
        hook_query(lua, &stage, "StageStats", "GetPlayerStageStats")?;
        hook_query(lua, &screenman, "ScreenManager", "GetTopScreen")
    };
    let result = install();
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() { trace.depth -= 1; }
    result
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SongLuaLivePlayer {
    pub possible_points: i32,
    pub actual_points: i32,
    pub current_possible_points: i32,
    pub life: f32,
    /// PlayerStageStats history is separate from the current LifeMeter value.
    pub life_record_latest: Option<f32>,
    pub failed: bool,
    pub combo: u32,
    pub miss_combo: u32,
    /// Native TapNoteScore order: None, HitMine, AvoidMine, CheckpointMiss,
    /// Miss, W5, W4, W3, W2, W1, CheckpointHit.
    pub tap_scores: [u32; 11],
    /// Actual gameplay feedback survives the native autoplay scoring bypass.
    pub last_judgment: Option<deadsync_gameplay::JudgmentRenderInfo>,
    pub hold_judgments: [Option<deadsync_gameplay::HoldJudgmentRenderInfo>; deadsync_core::input::MAX_COLS],
    pub autoplay: bool,
    /// Native ScreenGameplay suppresses Dead and Danger when FailType is Off.
    pub fail_enabled: bool,
}

pub(crate) struct LivePlayers([SongLuaLivePlayer; LUA_PLAYERS]);

#[cfg(feature = "test-support")]
struct OptionClockDeltas([f32; LUA_PLAYERS + 1]);

#[cfg(feature = "test-support")]
#[derive(Clone, Debug)]
pub struct SongLuaOverlayStageObservation {
    pub beat: f32,
    pub music_seconds: f32,
    pub players: [SongLuaLivePlayer; LUA_PLAYERS],
    pub player_step_indices: [Option<usize>; LUA_PLAYERS],
}

#[cfg(feature = "test-support")]
struct OverlayStageObserver {
    actor: usize,
    step_count: usize,
    observation: Option<SongLuaOverlayStageObservation>,
}

#[cfg(feature = "test-support")]
pub(crate) fn observe_overlay_stage(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some((owner, step_count)) = lua.app_data_ref::<OverlayStageObserver>()
        .map(|observer| (observer.actor, observer.step_count)) else { return Ok(()); };
    if owner != actor.to_pointer() as usize { return Ok(()); }
    let Some(players) = lua.app_data_ref::<LivePlayers>().map(|players| players.0)
        else { return Ok(()); };
    let player_step_indices = crate::host::selected_step_indices(lua, step_count)?;
    let (beat, music_seconds) = crate::compile_song_runtime_values(lua)?;
    if let Some(mut observer) = lua.app_data_mut::<OverlayStageObserver>() {
        observer.observation = Some(SongLuaOverlayStageObservation {
            beat, music_seconds, players, player_step_indices,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SongLuaLivePlayerWrite {
    SetLife { player: usize, life: f32 },
    RecordLife { player: usize, life: f32 },
    Fail { player: usize },
}

struct PlayerWrites(Vec<SongLuaLivePlayerWrite>);
struct LiveStats([Table; LUA_PLAYERS]);
#[derive(Default)]
struct HealthStates([Option<&'static str>; LUA_PLAYERS]);

pub(crate) fn bind_player_stats(lua: &Lua, players: [Table; LUA_PLAYERS]) {
    lua.set_app_data(LiveStats(players));
}

fn queue_player_write(lua: &Lua, write: SongLuaLivePlayerWrite) {
    if !active(lua) { return; }
    if let Some(mut writes) = lua.app_data_mut::<PlayerWrites>() {
        writes.0.push(write);
    } else {
        lua.set_app_data(PlayerWrites(vec![write]));
    }
}

pub(crate) fn player_life(lua: &Lua, player: usize) -> Option<f32> {
    lua.app_data_ref::<LivePlayers>().and_then(|players| players.0.get(player).map(|p| p.life))
}

pub(crate) fn player_autoplay(lua: &Lua, player: usize) -> Option<bool> {
    lua.app_data_ref::<LivePlayers>().and_then(|players| players.0.get(player).map(|p| p.autoplay))
}

pub(crate) fn player_health_state(lua: &Lua, player: usize) -> Option<&'static str> {
    lua.app_data_ref::<HealthStates>().and_then(|states| states.0.get(player).copied().flatten())
}

pub(crate) fn set_player_life(lua: &Lua, player: usize, life: f32) -> mlua::Result<()> {
    if let Some(mut values) = lua.app_data_mut::<LivePlayers>()
        && let Some(value) = values.0.get_mut(player) {
        value.life = life;
        value.life_record_latest = Some(life);
    }
    if let Some(stats) = lua.app_data_ref::<LiveStats>()
        .and_then(|stats| stats.0.get(player).cloned()) {
        stats.raw_set("__songlua_life", life)?;
    }
    queue_player_write(lua, SongLuaLivePlayerWrite::SetLife { player, life });
    Ok(())
}

fn record_player_life(lua: &Lua, player: usize, life: f32) -> mlua::Result<()> {
    if let Some(mut values) = lua.app_data_mut::<LivePlayers>()
        && let Some(value) = values.0.get_mut(player) { value.life_record_latest = Some(life); }
    if let Some(stats) = lua.app_data_ref::<LiveStats>()
        .and_then(|stats| stats.0.get(player).cloned()) {
        stats.raw_set("__songlua_life", life)?;
    }
    queue_player_write(lua, SongLuaLivePlayerWrite::RecordLife { player, life });
    Ok(())
}

pub(crate) fn fail_player(lua: &Lua, player: usize) {
    if let Some(mut values) = lua.app_data_mut::<LivePlayers>()
        && let Some(value) = values.0.get_mut(player) { value.failed = true; }
    queue_player_write(lua, SongLuaLivePlayerWrite::Fail { player });
}

fn apply_player_writes<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
    writes: impl IntoIterator<Item = SongLuaLivePlayerWrite>,
    game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
) {
    for write in writes {
        match write {
            SongLuaLivePlayerWrite::SetLife { player, life } => game.set_player_life_from_script(player, life),
            SongLuaLivePlayerWrite::RecordLife { player, life } => game.record_player_life_from_script(player, life),
            SongLuaLivePlayerWrite::Fail { player } => game.mark_player_stage_failed_from_script(player),
        }
    }
}

fn judgment_score(judgment: deadsync_rules::judgment::Judgment) -> &'static str {
    use deadsync_rules::judgment::JudgeGrade::*;
    match judgment.grade {
        Fantastic => "TapNoteScore_W1", Excellent => "TapNoteScore_W2",
        Great => "TapNoteScore_W3", Decent => "TapNoteScore_W4",
        WayOff => "TapNoteScore_W5", Miss => "TapNoteScore_Miss",
    }
}

fn live_tap_note(lua: &Lua, params: &Table, note: &deadsync_rules::note::Note) -> mlua::Result<Table> {
    use deadsync_core::note::NoteType;
    let table = lua.create_table()?;
    let kind = match note.note_type {
        NoteType::Tap => "TapNoteType_Tap", NoteType::Hold | NoteType::Roll => "TapNoteType_HoldHead",
        NoteType::Mine => "TapNoteType_Mine", NoteType::Lift => "TapNoteType_Lift",
        NoteType::Fake => "TapNoteType_Fake",
    };
    table.raw_set("TapNoteType", kind)?;
    table.raw_set("TapNoteSubType", if note.note_type == NoteType::Roll { "TapNoteSubType_Roll" }
        else { "TapNoteSubType_Hold" })?;
    table.raw_set("HoldDuration", note.hold.as_ref().map_or(0.0, |hold| hold.end_beat - note.beat))?;
    table.raw_set("KeysoundIndex", -1)?;
    table.raw_set("TapNoteScore", note.result.map_or("TapNoteScore_None", judgment_score))?;
    table.raw_set("TapNoteOffset", note.result.map_or(0.0, |result| result.time_error_ms / 1000.0))?;
    table.raw_set("Held", note.result.is_some_and(|result| result.miss_because_held))?;
    let table = crate::lua_util::normalize_tap_note_table(lua, params, table)?;
    // Ordinary dance notes have no assigned routine-player number.
    table.raw_set("GetPlayerNumber", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
    Ok(table)
}

fn tap_row_judgment_params<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
    lua: &Lua, game: &deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    player: usize, row_entry: usize, judgment: deadsync_rules::judgment::Judgment,
) -> mlua::Result<Table> {
    let row = &game.chart_runtime.row_entries[row_entry];
    let notes = &game.chart_runtime.notes;
    let columns = game.cols_per_player();
    let first_column = player * columns;
    let first_track = row.note_indices().iter().map(|index| notes[index.get()].column)
        .min().unwrap_or(first_column) - first_column;
    let params = lua.create_table()?;
    params.raw_set("Player", format!("PlayerNumber_P{}", player + 1))?;
    params.raw_set("FirstTrack", first_track)?;
    params.raw_set("TapNoteScore", judgment_score(judgment))?;
    params.raw_set("TapNoteOffset", judgment.time_error_ms / 1000.0)?;
    params.raw_set("Early", judgment.time_error_ms < 0.0)?;
    let early = row.note_indices().iter().map(|index| &notes[index.get()])
        .find(|note| note.result.is_some_and(|result| result.time_error_music_ns == judgment.time_error_music_ns
            && result.grade == judgment.grade)).and_then(|note| note.early_result);
    params.raw_set("EarlyTapNoteScore", early.map_or("TapNoteScore_None", judgment_score))?;
    params.raw_set("EarlyTapNoteOffset", early.map_or(0.0, |result| result.time_error_ms / 1000.0))?;
    if judgment.grade == deadsync_rules::judgment::JudgeGrade::Miss {
        params.raw_set("HeldMiss", judgment.miss_because_held)?;
    }
    let row_notes = lua.create_table()?;
    let holds = lua.create_table()?;
    // Native SetJudgment includes each note at this row, or the preceding
    // hold head when that track is still inside a hold. Owned lane indices
    // avoid scanning the song or retaining an event history on every frame.
    for column in first_column..first_column + columns {
        let lane = &game.chart_runtime.lane_indices.note_indices[column];
        let at = lane.partition_point(|index| notes[index.get()].row_index < row.row_index.get());
        if let Some(index) = lane.get(at)
            && notes[index.get()].row_index == row.row_index.get() {
            row_notes.raw_set(column - first_column + 1, live_tap_note(lua, &params, &notes[index.get()])?)?;
            continue;
        }
        let lane_holds = &game.chart_runtime.lane_indices.hold_indices[column];
        let before = lane_holds.partition_point(|index| notes[index.get()].row_index < row.row_index.get());
        if let Some(index) = before.checked_sub(1).and_then(|index| lane_holds.get(index)) {
            let note = &notes[index.get()];
            if note.hold.as_ref().is_some_and(|hold| hold.end_row_index >= row.row_index.get()) {
                holds.raw_set(column - first_column + 1, live_tap_note(lua, &params, note)?)?;
            }
        }
    }
    params.raw_set("Notes", row_notes)?;
    params.raw_set("Holds", holds)?;
    Ok(params)
}

fn finish_health_update(lua: &Lua) -> mlua::Result<()> {
    let screen: Table = lua.globals().raw_get("__songlua_top_screen")?;
    for player in 0..LUA_PLAYERS {
        if screen.raw_get::<Option<bool>>(if player == 0 { "__songlua_player1_enabled" }
            else { "__songlua_player2_enabled" })? == Some(false) { continue; }
        let Some(value) = lua.app_data_ref::<LivePlayers>().and_then(|values| values.0.get(player).copied())
            else { continue; };
        let fail_enabled = crate::song_tables::live_player_fail_type(lua, player, 3)?
            .map_or(value.fail_enabled, |mode| mode != deadsync_gameplay::GameplayFailType::Off);
        let state = if fail_enabled && value.life <= 0.0 { "HealthState_Dead" }
            else if value.life >= 1.0 { "HealthState_Hot" }
            else if fail_enabled && value.life < crate::SONG_LUA_DANGER_LIFE { "HealthState_Danger" }
            else { "HealthState_Alive" };
        let old = player_health_state(lua, player).unwrap_or("HealthState_Alive");
        if let Some(mut states) = lua.app_data_mut::<HealthStates>() { states.0[player] = Some(state); }
        if state != old {
            let params = lua.create_table()?;
            params.raw_set("PlayerNumber", format!("PlayerNumber_P{}", player + 1))?;
            params.raw_set("HealthState", state)?;
            params.raw_set("OldHealthState", old)?;
            crate::lua_util::broadcast_song_lua_message(lua, "HealthStateChanged", Some(Value::Table(params)))?;
        }
    }
    Ok(())
}

pub(crate) fn update_player_holds(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(player) = actor.raw_get::<Option<usize>>("__songlua_player_index")? else { return Ok(()); };
    let Some(value) = lua.app_data_ref::<LivePlayers>()
        .and_then(|values| values.0.get(player).copied()) else { return Ok(()); };
    let Some(frames) = actor.raw_get::<Option<Table>>("__songlua_hold_judgments")? else { return Ok(()); };
    for (column, render) in value.hold_judgments.iter().enumerate().take(frames.raw_len()) {
        let Some(render) = render else { continue; };
        let frame: Table = frames.raw_get(column + 1)?;
        let sprite: Table = crate::lua_util::actor_children(lua, &frame)?.raw_get("HoldJudgment")?;
        if sprite.raw_get::<Option<f32>>("__songlua_last_hold_seconds")? == Some(render.started_at_screen_s) {
            continue;
        }
        sprite.raw_set("__songlua_last_hold_seconds", render.started_at_screen_s)?;
        let state = match render.result {
            deadsync_rules::note::HoldResult::Held => 0,
            deadsync_rules::note::HoldResult::LetGo => 1,
            // Native MissedHold leaves the old animation intact; the
            // inherited Simply Love command is empty.
            deadsync_rules::note::HoldResult::Missed => continue,
        };
        crate::lua_util::ensure_tween_replay(lua, &sprite)?;
        // HoldJudgment::ResetAnimation, then Simply Love's Held/LetGo
        // metrics. Advance children before starting the new native queue.
        for method in ["stoptweening", "stopeffect"] {
            sprite.get::<Function>(method)?.call::<()>(sprite.clone())?;
        }
        sprite.get::<Function>("xy")?.call::<()>((sprite.clone(), 0.0, 0.0))?;
        sprite.get::<Function>("setstate")?.call::<()>((sprite.clone(), state))?;
        let white = lua.create_sequence_from([1.0_f32; 4])?;
        sprite.get::<Function>("diffuse")?.call::<()>((sprite.clone(), white.clone()))?;
        for (method, amount) in [("zoom", 0.4), ("linear", 0.3), ("zoom", 0.5),
                                 ("sleep", 0.5), ("linear", 0.0)] {
            sprite.get::<Function>(method)?.call::<()>((sprite.clone(), amount))?;
        }
        white.raw_set(4, 0.0_f32)?;
        sprite.get::<Function>("diffuse")?.call::<()>((sprite, white))?;
    }
    Ok(())
}

pub(crate) fn update_player_note_field_zoom(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(player) = actor.raw_get::<Option<usize>>("__songlua_player_index")? else { return Ok(()); };
    let Some(options) = lua.app_data_ref::<crate::song_tables::SongLuaOptionLevels>()
        .and_then(|levels| levels.players.get(player).map(|levels| levels[3].clone()))
        else { return Ok(()); };
    let mini = options.raw_get::<Option<Table>>("__songlua_player_option_state")?
        .map(|state| state.raw_get::<Option<f32>>("mini")).transpose()?.flatten().unwrap_or(0.0);
    let Some(field) = crate::actor_children(lua, actor)?.raw_get::<Option<Table>>("NoteField")?
        else { return Ok(()); };
    // Player::Update sets the NoteField's Mini zoom after ActorFrame::Update,
    // before SongForeground callbacks. OITG mode preserves its Z zoom.
    let zoom = 1.0 - mini * 0.5;
    for axis in ["zoom_x", "zoom_y"] {
        crate::lua_util::capture_block_set_f32(lua, &field, axis, zoom)?;
    }
    if !actor.raw_get::<Option<bool>>("__songlua_oitg_zoom_mode")?.unwrap_or(false) {
        crate::lua_util::capture_block_set_f32(lua, &field, "zoom_z", zoom)?;
    }
    Ok(())
}

pub(crate) fn update_player_judgment(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(player) = actor.raw_get::<Option<usize>>("__songlua_player_index")? else { return Ok(()); };
    let Some(value) = lua.app_data_ref::<LivePlayers>()
        .and_then(|values| values.0.get(player).copied()) else { return Ok(()); };
    let Some(render) = value.last_judgment else { return Ok(()); };
    if actor.raw_get::<Option<f32>>("__songlua_last_judgment_seconds")? == Some(render.started_at_screen_s) {
        return Ok(());
    }
    actor.raw_set("__songlua_last_judgment_seconds", render.started_at_screen_s)?;
    let children = crate::lua_util::actor_children(lua, actor)?;
    let Some(frame) = children.raw_get::<Option<Table>>("Judgment")? else { return Ok(()); };
    let children = crate::lua_util::actor_children(lua, &frame)?;
    let Some(sprite) = children.raw_get::<Option<Table>>("JudgmentWithOffsets")? else { return Ok(()); };
    crate::lua_util::ensure_tween_replay(lua, &sprite)?;
    let sl: Table = lua.globals().raw_get("SL")?;
    let mods: Table = sl.raw_get::<Table>(if player == 0 { "P1" } else { "P2" })?.raw_get("ActiveModifiers")?;
    let states = sprite.get::<Function>("GetNumStates")?.call::<usize>(sprite.clone())?;
    let mut index = deadsync_rules::judgment::judge_grade_ix(render.judgment.grade);
    if states == 7 || states == 14 {
        if index == 0 {
            if mods.raw_get::<Option<bool>>("ShowFaPlusWindow")?.unwrap_or(false)
                && render.judgment.window != Some(deadsync_rules::judgment::TimingWindow::W0)
                && !value.autoplay {
                index = 1;
            }
        } else { index += 1; }
    }
    if states == 12 || states == 14 {
        index = index * 2 + usize::from(render.judgment.time_error_ms >= 0.0);
    }
    // Simply Love Graphics/Player judgment.lua: Reset, show the earned
    // frame, and run the actual Sprite tween queue. No native observation
    // supplies the result or the duration of a gameplay judgment.
    for method in ["finishtweening", "stopeffect"] {
        sprite.get::<Function>(method)?.call::<()>(sprite.clone())?;
    }
    sprite.get::<Function>("visible")?.call::<()>((sprite.clone(), true))?;
    sprite.get::<Function>("setstate")?.call::<()>((sprite.clone(), index))?;
    if mods.raw_get::<Option<bool>>("JudgmentTilt")?.unwrap_or(false) {
        let offset = render.judgment.time_error_ms;
        let rotation = if render.judgment.grade == deadsync_rules::judgment::JudgeGrade::Miss { 0.0 }
            else { offset.signum() * (offset.abs() / 1000.0).min(0.050) * 300.0
                * mods.raw_get::<Option<f32>>("TiltMultiplier")?.unwrap_or(1.0) };
        sprite.get::<Function>("rotationz")?.call::<()>((sprite.clone(), rotation))?;
    }
    for (method, value) in [("zoom", 0.8), ("decelerate", 0.1), ("zoom", 0.75),
                            ("sleep", 0.6), ("accelerate", 0.2), ("zoom", 0.0)] {
        sprite.get::<Function>(method)?.call::<()>((sprite.clone(), value))?;
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub struct SongLuaLiveFrame {
    pub beat: f64,
    pub music_seconds: f64,
    pub screen_seconds: f64,
    pub delta_seconds: f64,
    /// Independent process clock; music can remain frozen while this advances.
    pub uptime_seconds: Option<f64>,
    /// The gameplay core reached its outro and removed active attacks.
    pub gameplay_outro: bool,
    /// The actual gameplay-owned attack schedule, independent of Lua writes.
    pub chart_attacks: Option<[std::sync::Arc<[deadsync_gameplay::ChartAttackWindow]>; LUA_PLAYERS]>,
    pub players: [SongLuaLivePlayer; LUA_PLAYERS],
}

impl SongLuaLivePlayer {
    pub fn from_player(player: &deadsync_gameplay::PlayerRuntime, possible_points: i32) -> Self {
        let [w1, w2, w3, w4, w5, miss] = player.judgment_counts;
        Self {
            possible_points, actual_points: player.earned_grade_points,
            current_possible_points: player.current_possible_points,
            life: player.life, life_record_latest: player.life_history.last().map(|&(_, life)| life),
            failed: player.stage_failed || player.fail_time.is_some(),
            fail_enabled: player.fail_type != deadsync_gameplay::GameplayFailType::Off,
            combo: player.combo, miss_combo: player.miss_combo,
            tap_scores: [0, player.mines_hit, player.mines_avoided, player.checkpoints_missed,
                miss, w5, w4, w3, w2, w1, player.checkpoints_hit],
            last_judgment: player.last_judgment,
            hold_judgments: [None; deadsync_core::input::MAX_COLS],
            autoplay: false,
        }
    }
}

impl SongLuaLiveFrame {
    /// Gameplay stores a sole cabinet P2 in runtime slot zero, as does the
    /// preparation context's Lua P1. No course carry or frozen display counts
    /// are substituted for the current stage's actual progress.
    pub fn from_gameplay<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        game: &deadsync_gameplay::GameplayRuntimeState<P, O, C, D>, delta_seconds: f32,
    ) -> Self {
        Self {
            beat: f64::from(game.clock.song_position.current_beat),
            music_seconds: game.clock.song_position.current_music_time_ns as f64 / 1_000_000_000.0,
            screen_seconds: f64::from(game.boundary.total_elapsed_in_screen),
            delta_seconds: f64::from(delta_seconds),
            uptime_seconds: None,
            gameplay_outro: game.mods.attacks.cleared_for_outro,
            chart_attacks: Some(game.mods.attacks.option_windows.clone()),
            players: std::array::from_fn(|player| game.stage_itg_score_inputs(player).map_or_else(
                SongLuaLivePlayer::default, |score| {
                    let mut value = SongLuaLivePlayer::from_player(
                        &game.players_runtime.players[player], score.possible_grade_points);
                    value.autoplay = game.live_autoplay_enabled();
                    let columns = game.cols_per_player();
                    for (column, render) in value.hold_judgments.iter_mut().enumerate().take(columns) {
                        *render = game.hold_judgment(player * columns + column);
                    }
                    value
                })),
        }
    }
}

fn feed_live_stats(stage: &Table, players: &[Table; LUA_PLAYERS], frame: &SongLuaLiveFrame) -> mlua::Result<()> {
    stage.raw_set("__songlua_gameplay_seconds", frame.screen_seconds)?;
    for (stats, value) in players.iter().zip(frame.players) {
        stats.raw_set("__songlua_possible_dance_points", value.possible_points)?;
        stats.raw_set("__songlua_actual_dance_points", value.actual_points)?;
        stats.raw_set("__songlua_current_possible_dance_points", value.current_possible_points)?;
        stats.raw_set("__songlua_failed", value.failed)?;
        stats.raw_set("__songlua_life", value.life_record_latest.unwrap_or(value.life))?;
        if let Some(meter) = stats.raw_get::<Option<Table>>("__songlua_life_meter")? {
            meter.raw_set("__songlua_life", value.life)?;
        }
        stats.raw_set("__songlua_combo", value.combo)?;
        stats.raw_set("__songlua_miss_combo", value.miss_combo)?;
        let counts = stats.raw_get::<Table>("__songlua_tap_scores")?;
        for (name, count) in TAP_SCORES.into_iter().zip(value.tap_scores) {
            counts.raw_set(name, count)?;
        }
    }
    Ok(())
}

fn invoke_note_field_tap_callback(
    lua: &Lua, field: &Table, callback: &Function,
    mut tap: deadsync_gameplay::GameplayNoteFieldTap, columns: usize,
) -> mlua::Result<deadsync_gameplay::GameplayNoteFieldTap> {
    // NoteField.cpp passes three arguments without self and reads three optional
    // returns. Each valid return replaces its argument independently.
    let result = callback.call::<MultiValue>((tap.column + 1, format!("TapNoteScore_{}", tap.score), tap.bright));
    let values = result?;
    if let Some(value) = values.front() {
        if let Some(column) = lua.coerce_number(value.clone())? {
            // Native subtracts one before truncating the floating Lua number.
            let column = (column - 1.0).trunc();
            if column.is_finite() && column >= 0.0 && column < columns as f64 {
                tap.column = column as usize;
            } else {
                crate::lua_util::report_update_error(lua, field,
                    "__songlua_did_tap_note_column_error_reported", "DidTapNote callback return",
                    &mlua::Error::runtime(format!("Column returned by callback must be between 1 and {columns}")))?;
            }
        }
    }
    if let Some(value) = values.get(1).filter(|value| !matches!(value, Value::Nil)) {
        let score = match value {
            Value::Integer(index) => usize::try_from(*index).ok().and_then(|index| TAP_SCORES.get(index).copied()),
            Value::Number(index) if index.is_finite() && *index >= 0.0 && index.fract() == 0.0 =>
                TAP_SCORES.get(*index as usize).copied(),
            Value::String(score) => {
                let score = score.to_str()?;
                TAP_SCORES.iter().copied().find(|name|
                    score.as_ref() == format!("TapNoteScore_{name}") || score.eq_ignore_ascii_case(name))
            }
            _ => None,
        };
        if let Some(score) = score { tap.score = score; }
        else {
            crate::lua_util::report_update_error(lua, field,
                "__songlua_did_tap_note_score_error_reported", "DidTapNote callback return",
                &mlua::Error::runtime("Expected TapNoteScore"))?;
        }
    }
    if let Some(Value::Boolean(bright)) = values.get(2) { tap.bright = *bright; }
    Ok(tap)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SongLuaLiveRequest {
    NewScreen(String),
    Transition(String),
}

#[derive(Debug)]
pub struct SongLuaLiveSourceRequest {
    pub music_seconds: f64,
    pub request: SongLuaLiveRequest,
}

pub(crate) struct TransitionRequest(pub String);

pub(crate) struct SourceRequests(pub Vec<SongLuaLiveSourceRequest>);

pub(crate) fn capture_source_request(lua: &Lua, request: SongLuaLiveRequest) -> mlua::Result<()> {
    if lua.app_data_ref::<SourceRequests>().is_none() { return Ok(()); }
    let music_seconds = f64::from(crate::compile_song_runtime_values(lua)?.1);
    if let Some(mut events) = lua.app_data_mut::<SourceRequests>() {
        events.0.push(SongLuaLiveSourceRequest { music_seconds, request });
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub enum SongLuaLiveTarget {
    Root(usize),
    Background(usize, usize),
    Foreground(usize, usize),
}

#[derive(Debug)]
pub struct SongLuaLiveBroadcast {
    pub beat: f32,
    pub music_seconds: f64,
    pub name: String,
    /// Snapshot before listeners run. Unsupported trace values remain an
    /// explicit diagnostic gap; they do not interrupt message delivery.
    pub params: Result<Option<serde_json::Value>, String>,
}

#[derive(Debug, Default)]
pub struct SongLuaLiveOutput {
    pub request: Option<SongLuaLiveRequest>,
    /// Authored calls retain their multiplicity while the screen manager
    /// consumes only the final queued screen target.
    pub source_requests: Vec<SongLuaLiveSourceRequest>,
    pub broadcasts: Vec<(f32, String, bool)>,
    /// Explicit Lua MESSAGEMAN calls, including calls from engine-message
    /// listeners. Native source traces omit the engine-generated messages.
    pub source_broadcasts: Vec<SongLuaLiveBroadcast>,
    pub sounds: Vec<crate::SongLuaSoundEvent>,
    /// Native-style callback errors retain partial state and are reported once.
    pub errors: Vec<String>,
    /// Missing observational inputs, separate from callback execution errors.
    pub coverage_gaps: Vec<String>,
    pub player_writes: Vec<SongLuaLivePlayerWrite>,
    /// Frozen command/update Draw submissions, in their execution order.
    pub draw_frames: Vec<crate::SongLuaDrawFrame>,
}

impl SongLuaLiveOutput {
    pub fn append(&mut self, mut next: Self) {
        if next.request.is_some() { self.request = next.request; }
        self.source_requests.append(&mut next.source_requests);
        self.broadcasts.append(&mut next.broadcasts);
        self.source_broadcasts.append(&mut next.source_broadcasts);
        self.sounds.append(&mut next.sounds);
        self.errors.append(&mut next.errors);
        self.coverage_gaps.append(&mut next.coverage_gaps);
        self.player_writes.append(&mut next.player_writes);
        self.draw_frames.append(&mut next.draw_frames);
    }
}

/// Owns one initialized Lua scene on the gameplay/screen thread. Worker
/// preparation transfers ownership once; the VM is never cloned. Actor tables
/// and snapshot buffers live for the screen and are sized during preparation.
/// Tween queues retain only pending steps, not a whole-song sample history.
/// Advancement runs authored callbacks without speculative future probes.
/// Asset loads requested by those callbacks still require runtime integration.
pub struct SongLuaLiveScene {
    pub(crate) lua: Lua,
    root: Value,
    stage: Table,
    players: [Table; LUA_PLAYERS],
    pub(crate) actors: Vec<Table>,
    pub(crate) states: Vec<SongLuaOverlayState>,
    pub(crate) tracked: Vec<(SongLuaTrackedActorTarget, Table, SongLuaOverlayState)>,
    pub(crate) registry: Vec<Table>,
    targets: Vec<(usize, SongLuaLiveTarget)>,
    visual_targets: rustc_hash::FxHashMap<usize, SongLuaLiveTarget>,
    context: SongLuaCompileContext,
    pub(crate) entry_paths: Vec<std::path::PathBuf>,
    pub(crate) primary_index: usize,
    last_beat: f64,
    last_music_seconds: f32,
    prepared_frame_beat: Option<f64>,
    backgrounds: Vec<LiveBackground>,
    background_layers: [LiveBackgroundLayer; 2],
    background_order: Vec<usize>,
    foregrounds: Vec<LiveForeground>,
    pub(crate) startup_output: SongLuaLiveOutput,
    outro_started: bool,
    outro_options_reset: bool,
    outro_off_pending: bool,
    outro_failed: bool,
    ended: bool,
    updated: bool,
    chart_attacks: [LiveChartAttacks; LUA_PLAYERS],
}

impl fmt::Debug for SongLuaLiveScene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SongLuaLiveScene")
            .field("actors", &self.actors.len())
            .field("tracked", &self.tracked.len())
            .finish_non_exhaustive()
    }
}

impl SongLuaLiveScene {
    /// Independent ModsGroup timer inputs, observed before the next screen update.
    #[cfg(feature = "test-support")]
    pub fn set_option_clock_deltas(&mut self, deltas: [f32; LUA_PLAYERS + 1]) {
        self.lua.set_app_data(OptionClockDeltas(deltas));
    }

    /// Raw RageTimer reads at the native ArrowEffects update phase.
    #[cfg(feature = "test-support")]
    pub fn set_arrow_clock_inputs(&mut self, tipsy: [Option<f64>; LUA_PLAYERS], tan_tipsy: [Option<f64>; LUA_PLAYERS]) {
        self.lua.set_app_data(crate::clock_inputs::ArrowFrameClocks { tipsy, tan_tipsy });
    }

    /// ScreenManager dispatches input after the screen's own input handler.
    /// Callback return values only control the remaining Lua callbacks.
    pub fn input(&mut self, event: &crate::SongLuaInputEvent<'_>)
        -> mlua::Result<(bool, SongLuaLiveOutput)>
    {
        if self.ended { return Err(mlua::Error::runtime("cannot send input to an ended song Lua scene")); }
        let (handled, errors) = crate::input::dispatch(&self.lua, event)?;
        for (actor, state) in self.actors.iter().zip(&mut self.states) {
            *state = read_state(&self.lua, actor)?;
        }
        for (_, actor, state) in &mut self.tracked { *state = read_state(&self.lua, actor)?; }
        let mut output = take_output(&self.lua)?;
        output.errors.extend(errors);
        Ok((handled, output))
    }

    /// Enable a read-only observation at the native gameplay overlay probe phase.
    #[cfg(feature = "test-support")]
    pub fn enable_overlay_stage_observer(&self) -> mlua::Result<()> {
        let screen: Table = self.lua.globals().raw_get("__songlua_top_screen")?;
        let children = crate::actor_children(&self.lua, &screen)?;
        let overlay: Table = children.raw_get("Overlay")?;
        let owner = crate::actor_children(&self.lua, &overlay)?
            .raw_get::<Option<Table>>("ITGmaniaHarnessTraceProbe")?.unwrap_or(overlay);
        self.lua.set_app_data(OverlayStageObserver {
            actor: owner.to_pointer() as usize,
            step_count: self.context.song_steps.len(), observation: None,
        });
        Ok(())
    }

    #[cfg(feature = "test-support")]
    pub fn take_overlay_stage_observation(&self) -> Option<SongLuaOverlayStageObservation> {
        self.lua.app_data_mut::<OverlayStageObserver>()
            .and_then(|mut observer| observer.observation.take())
    }

    pub(crate) fn new(
        lua: Lua,
        root: Value,
        context: SongLuaCompileContext,
        entry_paths: Vec<std::path::PathBuf>,
        primary_index: usize,
        actors: Vec<Table>,
        tracked: Vec<(SongLuaTrackedActorTarget, Table, SongLuaOverlayState)>,
        mut startup_broadcasts: Vec<(f32, String, bool)>,
    ) -> mlua::Result<Self> {
        let manager = lua.globals().get::<Table>("STATSMAN")?;
        let stage = manager.get::<Function>("GetCurStageStats")?.call::<Table>(manager)?;
        let players = [
            stage.get::<Function>("GetPlayerStageStats")?.call::<Table>((stage.clone(), "PlayerNumber_P1"))?,
            stage.get::<Function>("GetPlayerStageStats")?.call::<Table>((stage.clone(), "PlayerNumber_P2"))?,
        ];
        lua.set_app_data(LiveStats(players.clone()));
        lua.set_app_data(HealthStates::default());
        let screen: Table = lua.globals().raw_get("__songlua_top_screen")?;
        let children = crate::actor_children(&lua, &screen)?;
        for (player, stats) in players.iter().enumerate() {
            if let Some(meter) = children.raw_get::<Option<Table>>(format!("LifeP{}", player + 1))? {
                stats.raw_set("__songlua_life_meter", meter)?;
            }
        }
        let registry = crate::lua_util::prepare_live_actor_handle_cache(
            &lua, actors.iter().cloned().chain(tracked.iter().map(|(_, actor, _)| actor.clone())),
        )?;
        crate::lua_util::prepare_live_updates(&lua, &root)?;
        let states = actors.iter().map(|actor| read_state(&lua, actor))
            .collect::<mlua::Result<Vec<_>>>()?;
        let mut draw_sources = actors.iter().enumerate().map(|(index, actor)|
            (actor.to_pointer() as usize, crate::draw_capture::DrawSource::Overlay(index)))
            .collect::<rustc_hash::FxHashMap<_, _>>();
        for (target, actor, _) in &tracked {
            use crate::draw_capture::DrawSource as S;
            use SongLuaTrackedActorTarget as T;
            let source = match *target {
                T::Player(p) => S::Player(p), T::PlayerJudgment(p) => S::Judgment(p),
                T::PlayerCombo(p) => S::Combo(p), T::SongForeground => S::SongForeground,
                T::ScreenLayer(layer) => S::ScreenLayer(layer),
            };
            draw_sources.insert(actor.to_pointer() as usize, source);
        }
        lua.set_app_data(crate::draw_capture::LiveSources(draw_sources));
        let mut startup_output = take_output(&lua)?;
        startup_broadcasts.append(&mut startup_output.broadcasts);
        startup_output.broadcasts = startup_broadcasts;
        let last_beat = f64::from(crate::compile_song_runtime_values(&lua)?.0);
        let mut backgrounds = Vec::new();
        let mut background_layers: [LiveBackgroundLayer; 2] = std::array::from_fn(|_| LiveBackgroundLayer::default());
        let mut foregrounds = Vec::new();
        if let Value::Table(roots) = &root {
            for actor in roots.sequence_values::<Value>() {
                let Value::Table(actor) = actor? else { continue; };
                if let Some(start_beat) = actor.raw_get::<Option<f32>>("__songlua_background_start_beat")? {
                    let index = backgrounds.len();
                    let (layer, changes) = context.background_activations.get(index).cloned()
                        .unwrap_or_else(|| (0, vec![deadsync_chart::song::SongBackgroundLuaActivation {
                            start_beat, rate: 1.0, transition: String::new(),
                        }]));
                    let layer = background_layers.get_mut(usize::from(layer))
                        .ok_or_else(|| mlua::Error::runtime("invalid background layer"))?;
                    layer.changes.extend(changes.into_iter().map(|change| (index, change)));
                    backgrounds.push(LiveBackground { actor: actor.clone() });
                }
                if let Some(start_beat) = actor.raw_get::<Option<f32>>("__songlua_foreground_start_beat")? {
                    foregrounds.push(LiveForeground { actor, start_beat, stop_beat: 0.0, started: false, finished: false });
                }
            }
        }
        for layer in &mut background_layers { layer.changes.sort_by(|a,b| a.1.start_beat.total_cmp(&b.1.start_beat)); }
        let background_order = (0..backgrounds.len()).collect();
        Ok(Self { lua, root, stage, players, actors, states, tracked, registry,
            targets: Vec::new(), visual_targets: Default::default(), context, entry_paths, primary_index, last_beat,
            last_music_seconds: -9999.0, prepared_frame_beat: None, backgrounds, background_layers, background_order, foregrounds, startup_output,
            outro_started: false, outro_options_reset: false, outro_off_pending: false, outro_failed: false,
            ended: false, updated: false,
            chart_attacks: std::array::from_fn(|_| LiveChartAttacks::default()) })
    }

    /// Advance one complete active-screen frame: update, then draw.
    pub fn advance(&mut self, frame: SongLuaLiveFrame) -> mlua::Result<SongLuaLiveOutput> {
        let mut output = self.update(frame)?;
        output.append(self.draw()?);
        Ok(output)
    }

    /// Update the owned actor tree and health. Screen owners can process exit
    /// or replacement before deciding whether this scene is still drawn.
    pub fn update(&mut self, frame: SongLuaLiveFrame) -> mlua::Result<SongLuaLiveOutput> {
        if self.ended { return Err(mlua::Error::runtime("cannot advance an ended song Lua scene")); }
        #[cfg(feature = "test-support")]
        if let Some(mut observer) = self.lua.app_data_mut::<OverlayStageObserver>() {
            observer.observation = None;
        }
        let beat = if let Some(beat) = self.prepared_frame_beat.take() {
            self.feed_stats(&frame)?;
            // Player callbacks refresh their own snapshots and can leave a
            // zero-delta capture frame. Resume the screen's sampled clock and
            // the final Player feedback before traversing the actor tree.
            crate::set_compile_song_runtime_values(&self.lua, beat as f32, frame.music_seconds as f32)?;
            crate::lua_util::set_live_frame(&self.lua, frame.screen_seconds, beat, frame.delta_seconds);
            self.lua.set_app_data(LivePlayers(frame.players));
            beat
        } else { self.prepare_frame(&frame)? };
        let lua = self.lua.clone();
        let root = self.root.clone();
        crate::lua_util::run_actor_compile_update_functions_with_entry(
            &lua, &root, frame.delta_seconds, |actor| {
                // Foreground is sorted after the screen's underlay and Player
                // actors. Its On callbacks observe their current-frame queues.
                self.advance_foreground(actor, beat as f32, frame.music_seconds as f32)
            })?;
        for foreground in &mut self.foregrounds {
            if foreground.started && !foreground.finished && beat as f32 > foreground.stop_beat {
                foreground.actor.get::<Function>("visible")?.call::<()>((foreground.actor.clone(), false))?;
                foreground.actor.raw_set("__songlua_live_inactive", true)?;
                foreground.finished = true;
            }
        }
        // ScreenGameplay posts NotesEnded after its actor Update. Screen
        // dispatches it after the following actor Update, then LeaveGameplay
        // removes attacks. Authored callbacks first see the rebuilt options
        // on the next frame. Use the gameplay boundary, not a trace timestamp.
        if self.outro_off_pending {
            self.tween_off_screen()?;
            self.outro_off_pending = false;
        }
        if self.outro_started && !self.outro_options_reset {
            crate::song_tables::clear_active_attack_options(&self.lua)?;
            self.outro_options_reset = true;
            // LeaveGameplay tweens successful screens off immediately. For
            // AllFailed it posts BeginFailed, handled after the next Update.
            let mut all_failed = true;
            for (index, (player, context)) in self.players.iter().zip(&self.context.players).enumerate() {
                if context.enabled {
                    if player.raw_get::<bool>("__songlua_failed")? {
                        // NotesEnded records failure in PlayerStageStats before
                        // LeaveGameplay/Off. It does not empty the LifeMeter.
                        record_player_life(&self.lua, index, 0.0)?;
                    } else {
                        all_failed = false;
                    }
                }
            }
            self.outro_failed = all_failed;
            if all_failed { self.outro_off_pending = true; }
            else { self.tween_off_screen()?; }
        }
        self.outro_started |= frame.gameplay_outro;
        self.last_music_seconds = frame.music_seconds as f32;
        for (actor, state) in self.actors.iter().zip(&mut self.states) {
            *state = read_state(&self.lua, actor)?;
        }
        for (_, actor, state) in &mut self.tracked {
            *state = read_state(&self.lua, actor)?;
        }
        // Capture blocks are compiler audit history. Actual tween destinations
        // and commands live in the bounded pending queue; discard old blocks.
        for actor in &self.registry {
            crate::reset_actor_capture(&self.lua, actor)?;
        }
        // ScreenGameplay updates health after its actor tree. GetHealthState
        // during a SetLife/LifeChanged callback still sees the previous state.
        if let Err(error) = finish_health_update(&self.lua) {
            let screen: Table = self.lua.globals().raw_get("__songlua_top_screen")?;
            crate::lua_util::report_update_error(&self.lua, &screen,
                "__songlua_health_error_reported", "HealthStateChanged", &error)?;
        }
        crate::host::update_native_arrow_effects(&self.lua)?;
        self.updated = true;
        take_output(&self.lua)
    }

    /// Prepare GameState and background actors before Player judgment callbacks.
    pub fn prepare_gameplay_frame(&mut self, frame: SongLuaLiveFrame) -> mlua::Result<()> {
        if self.prepared_frame_beat.is_none() {
            self.prepared_frame_beat = Some(self.prepare_frame(&frame)?);
        }
        Ok(())
    }

    fn prepare_frame(&mut self, frame: &SongLuaLiveFrame) -> mlua::Result<f64> {
        // GameState.cpp::UpdateSongPosition maintains global Song timing apart
        // from each Player's selected Steps timing. The domain frame carries
        // the player's beat; Lua's GAMESTATE and music effects use the song.
        let music_rate = crate::song_tables::current_music_rate(&self.lua)?;
        let position = self.context.song_timing.as_ref()
            .map(|timing| timing.get_song_position_with_rate(frame.music_seconds as f32, music_rate));
        let beat = position.as_ref().map_or(frame.beat, |position| f64::from(position.beat));
        self.feed_stats(frame)?;
        let runtime = self.lua.globals().get::<Table>(crate::SONG_LUA_RUNTIME_KEY)?;
        let uptime = frame.uptime_seconds.unwrap_or(
            runtime.get::<Option<f64>>("__songlua_uptime_seconds")?.unwrap_or(0.0)
                + frame.delta_seconds);
        runtime.set("__songlua_uptime_seconds", uptime)?;
        crate::set_compile_song_runtime_values(&self.lua, beat as f32, frame.music_seconds as f32)?;
        if let Some(position) = position {
            let runtime = self.lua.globals().get::<Table>(crate::SONG_LUA_RUNTIME_KEY)?;
            runtime.set(crate::SONG_LUA_RUNTIME_BPS_KEY, position.bpm / 60.0)?;
            runtime.set("__songlua_freeze", position.is_in_freeze)?;
            runtime.set("__songlua_delay", position.is_in_delay)?;
        } else {
            runtime.set(crate::SONG_LUA_RUNTIME_BPS_KEY, crate::song_bps_at_beat(beat as f32, &self.context))?;
        }
        // GameState::UpdateSongPosition updates each enabled PlayerState
        // using its selected Steps timing at the same music timestamp.
        let positions: Table = runtime.raw_get(crate::runtime::PLAYER_SONG_POSITIONS_KEY)?;
        let mut attack_positions = [0.0_f32; LUA_PLAYERS];
        for player in 0..LUA_PLAYERS {
            if !self.context.players[player].enabled { continue; }
            let owned: Table = positions.raw_get(player + 1)?;
            // GameState updates attack activation before ScreenGameplay samples
            // this frame's audio position; PlayerState still has the prior one.
            attack_positions[player] = owned.raw_get(crate::SONG_LUA_RUNTIME_SECONDS_KEY)?;
            let position = self.context.player_timing[player].as_ref()
                .or(self.context.song_timing.as_ref())
                .map(|timing| timing.get_song_position_with_rate(frame.music_seconds as f32, music_rate));
            owned.raw_set(crate::SONG_LUA_RUNTIME_SECONDS_KEY, frame.music_seconds as f32)?;
            owned.raw_set(crate::SONG_LUA_RUNTIME_BEAT_KEY,
                position.as_ref().map_or(frame.beat as f32, |position| position.beat))?;
            owned.raw_set(crate::SONG_LUA_RUNTIME_BPS_KEY, position.as_ref()
                .map_or(runtime.raw_get::<f32>(crate::SONG_LUA_RUNTIME_BPS_KEY)?,
                    |position| position.bpm / 60.0))?;
            owned.raw_set("__songlua_freeze", position.as_ref().is_some_and(|position| position.is_in_freeze))?;
            owned.raw_set("__songlua_delay", position.as_ref().is_some_and(|position| position.is_in_delay))?;
        }
        crate::set_compile_song_runtime_delta_values(&self.lua,
            (beat - self.last_beat) as f32, frame.delta_seconds as f32)?;
        self.last_beat = beat;
        crate::lua_util::set_live_frame(&self.lua, frame.screen_seconds, beat, frame.delta_seconds);
        self.lua.set_app_data(LivePlayers(frame.players));
        if !self.outro_started && let Some(windows) = &frame.chart_attacks {
            let master = self.context.players.iter().position(|player| player.enabled);
            for (player, attacks) in self.chart_attacks.iter_mut().enumerate() {
                attacks.update(&self.lua, player, master == Some(player),
                    &windows[player], attack_positions[player])?;
            }
        }
        let option_deltas: Option<[f32; LUA_PLAYERS + 1]> = None;
        #[cfg(feature = "test-support")]
        let option_deltas = self.lua.remove_app_data::<OptionClockDeltas>()
            .map(|inputs| inputs.0).or(option_deltas);
        if let Some(deltas) = option_deltas {
            crate::song_tables::advance_option_levels_with_clock_deltas(
                &self.lua, frame.screen_seconds, deltas)?;
        } else {
            crate::song_tables::advance_option_levels_with_frame_delta(
                &self.lua, frame.screen_seconds, frame.delta_seconds as f32)?;
        }
        self.advance_backgrounds(beat as f32, frame.music_seconds as f32)?;
        Ok(beat)
    }

    /// False until an authored draw callback has been installed. Removing one
    /// can leave this true; that only retains the ordinary draw traversal.
    pub fn has_custom_draw(&self) -> mlua::Result<bool> {
        Ok(self.lua.globals().raw_get::<Option<bool>>(crate::draw_capture::DRAW_INSTALLED_KEY)?.unwrap_or(false))
    }

    /// ScreenManager::Draw skips a screen before its first Update and after
    /// it has been popped. Authored Draw uses the latest owned tween pose.
    pub fn draw(&mut self) -> mlua::Result<SongLuaLiveOutput> {
        if self.ended || !self.updated { return Ok(SongLuaLiveOutput::default()); }
        // Ordinary drawing consumes the poses already read by Update/Input.
        // Only authored Draw callbacks can mutate them again in this phase.
        if !self.has_custom_draw()? { return take_output(&self.lua); }
        if let Value::Table(root) = &self.root {
            crate::lua_util::run_actor_draw_functions_for_table(&self.lua, root)?;
        }
        for (actor, state) in self.actors.iter().zip(&mut self.states) {
            *state = read_state(&self.lua, actor)?;
        }
        for (_, actor, state) in &mut self.tracked {
            *state = read_state(&self.lua, actor)?;
        }
        for actor in &self.registry { crate::reset_actor_capture(&self.lua, actor)?; }
        take_output(&self.lua)
    }

    fn advance_backgrounds(&mut self, beat: f32, music_seconds: f32) -> mlua::Result<()> {
        if self.backgrounds.is_empty() { return Ok(()); }
        let prefs = self.lua.globals().get::<Table>("PREFSMAN")?;
        let rate_tweens = prefs.get::<Function>("GetPreference")?
            .call::<Option<bool>>((prefs, "RateModsAffectFGChanges"))?.unwrap_or(false);
        let game = self.lua.globals().get::<Table>("GAMESTATE")?;
        let options = game.get::<Function>("GetSongOptionsObject")?
            .call::<Table>((game, "ModsLevel_Current"))?;
        let rate = if rate_tweens { 1.0 } else { options.get::<Function>("MusicRate")?.call::<f32>(options)? };
        for background in &self.backgrounds { background.actor.raw_set("__songlua_live_inactive", true)?; }
        let mut order = Vec::new();
        for layer in &mut self.background_layers {
            let next = layer.changes.iter().rposition(|(_, change)| change.start_beat <= beat);
            let mut delta = music_seconds - self.last_music_seconds;
            if let Some(next) = next.filter(|next| Some(*next) != layer.change_index) {
                let (owner, change) = &layer.changes[next];
                layer.fading = layer.current.filter(|current| current != owner);
                if let Some(fading) = layer.fading {
                    let actor = &self.backgrounds[fading].actor;
                    actor.get::<Function>("playcommand")?.call::<()>((actor.clone(), "LoseFocus"))?;
                    crate::lua_util::run_background_transition(&self.lua, actor, &change.transition)?;
                }
                layer.current = Some(*owner);
                layer.change_index = Some(next);
                let actor = &self.backgrounds[*owner].actor;
                crate::lua_util::init_actor_native_state(&self.lua, actor)?;
                actor.raw_set("__songlua_defer_on", false)?;
                crate::lua_util::run_actor_startup_commands_for_table(&self.lua, actor)?;
                actor.get::<Function>("playcommand")?.call::<()>((actor.clone(), "GainFocus"))?;
                let start = self.context.song_timing.as_ref().map_or_else(
                    || crate::song_elapsed_seconds_at(change.start_beat, &self.context) * crate::song_music_rate(&self.context),
                    |timing| timing.get_time_for_beat(change.start_beat));
                delta = music_seconds - start;
            }
            if let Some(fading) = layer.fading
                && crate::lua_util::actor_tween_time_left(&self.lua, &self.backgrounds[fading].actor)? == 0.0 {
                layer.fading = None;
            }
            let delta = (delta / rate).max(0.0);
            for (index, owner) in [layer.current, layer.fading].into_iter().enumerate() {
                let Some(owner) = owner else { continue; };
                let actor = &self.backgrounds[owner].actor;
                // SetUpdateRate belongs to the native effect wrapper, outside
                // any authored ActorFrame update rate on its loaded child.
                let owner_rate = if index == 0 {
                    layer.changes[layer.change_index.expect("current activation")].1.rate
                } else {
                    actor.raw_get::<Option<f32>>("__songlua_background_rate")?.unwrap_or(1.0)
                };
                actor.raw_set("__songlua_background_rate", owner_rate)?;
                actor.raw_set("__songlua_live_delta", f64::from(delta * owner_rate))?;
                actor.raw_set("__songlua_live_inactive", false)?;
                order.push(owner);
            }
        }
        for owner in 0..self.backgrounds.len() { if !order.contains(&owner) { order.push(owner); } }
        if order != self.background_order {
            if let Value::Table(root) = &self.root {
                for (position, &owner) in order.iter().enumerate() { root.raw_set(position + 1, self.backgrounds[owner].actor.clone())?; }
            }
            self.background_order = order;
            crate::lua_util::invalidate_compile_update_plan(&self.lua);
        }
        Ok(())
    }

    fn advance_foreground(&mut self, actor: &Table, beat: f32, music_seconds: f32) -> mlua::Result<()> {
        let Some(foreground) = self.foregrounds.iter_mut()
            .find(|foreground| foreground.actor.to_pointer() == actor.to_pointer())
            else { return Ok(()); };
        let prefs = self.lua.globals().get::<Table>("PREFSMAN")?;
        let rate_tweens = prefs.get::<Function>("GetPreference")?
            .call::<Option<bool>>((prefs, "RateModsAffectFGChanges"))?.unwrap_or(false);
        let game = self.lua.globals().get::<Table>("GAMESTATE")?;
        let options = game.get::<Function>("GetSongOptionsObject")?
            .call::<Table>((game, "ModsLevel_Current"))?;
        let music_rate = options.get::<Function>("MusicRate")?.call::<f32>(options.clone())?;
        let rate = if rate_tweens { 1.0 } else { music_rate };
        {
            let inactive = foreground.finished || beat < foreground.start_beat;
            foreground.actor.raw_set("__songlua_live_inactive", inactive)?;
            if inactive { return Ok(()); }
            let visible = foreground.actor.get::<Function>("GetVisible")?.call::<bool>(foreground.actor.clone())?;
            let delta = if !visible {
                foreground.actor.get::<Function>("visible")?.call::<()>((foreground.actor.clone(), true))?;
                foreground.actor.raw_set("__songlua_defer_on", false)?;
                if foreground.started {
                    foreground.actor.get::<Function>("playcommand")?.call::<()>((foreground.actor.clone(), "On"))?;
                } else {
                    crate::lua_util::run_actor_startup_commands_for_table(&self.lua, &foreground.actor)?;
                }
                // Native TimingData reads the current rate after foreground On.
                let start_music_rate = options.get::<Function>("MusicRate")?.call::<f32>(options.clone())?;
                let start = self.context.song_timing.as_ref().map_or_else(
                    || crate::song_elapsed_seconds_at(foreground.start_beat, &self.context) * crate::song_music_rate(&self.context),
                    |timing| timing.get_song_time_for_beat(foreground.start_beat, start_music_rate));
                let stop = start + crate::lua_util::actor_tween_time_left(&self.lua, &foreground.actor)?;
                foreground.stop_beat = self.context.song_timing.as_ref().map_or_else(
                    || crate::song_beat_at_elapsed_seconds(stop / crate::song_music_rate(&self.context), &self.context),
                    |timing| timing.get_song_position(stop).beat);
                foreground.started = true;
                // Foreground::Update consumes the music elapsed since the
                // authored start, including a first frame that crosses it.
                (music_seconds - start).max(0.0) / rate
            } else {
                (music_seconds - self.last_music_seconds).max(0.0) / rate
            };
            foreground.actor.raw_set("__songlua_live_delta", f64::from(delta))?;
        }
        Ok(())
    }

    fn feed_stats(&self, frame: &SongLuaLiveFrame) -> mlua::Result<()> {
        feed_live_stats(&self.stage, &self.players, frame)
    }

    fn tween_off_screen(&self) -> mlua::Result<()> {
        let screen: Table = self.lua.globals().raw_get("__songlua_top_screen")?;
        crate::lua_util::run_named_command_on_children_recursively(
            &self.lua, &screen, "OffCommand", None)?;
        if let Some(transition) = crate::actor_children(&self.lua, &screen)?.raw_get::<Option<Table>>("Out")? {
            crate::lua_util::start_actor_transition(&self.lua, &transition)?;
        }
        Ok(())
    }

    /// Share the retained VM with the gameplay thread's synchronous NoteField
    /// hook. No VM or actor tree is copied and each screen replaces its hook.
    pub fn install_note_field_tap_callback<P, O, C, D>(
        &self,
        game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
        bright_combo_threshold: u32,
    ) -> mlua::Result<()>
    where P: deadsync_gameplay::GameplayProfileData {
        let lua = self.lua.clone();
        let screen = lua.globals().get::<Table>("__songlua_top_screen")?;
        let children = crate::actor_children(&lua, &screen)?;
        let mut note_fields = [None, None];
        for (player, field) in note_fields.iter_mut().enumerate() {
            if let Some(actor) = children.get::<Option<Table>>(format!("PlayerP{}", player + 1))? {
                *field = crate::actor_children(&lua, &actor)?.get::<Option<Table>>("NoteField")?;
            }
        }
        let stage = self.stage.clone();
        let players = self.players.clone();
        let song_timing = self.context.song_timing.clone();
        game.note_field_tap_callback = Some(Box::new(move |game, tap| {
            let Some(field) = note_fields.get(tap.player).and_then(Option::as_ref) else { return tap; };
            let result = (|| -> mlua::Result<deadsync_gameplay::GameplayNoteFieldTap> {
                let Some(callback) = field.raw_get::<Option<Function>>("__songlua_did_tap_note_callback")?
                    else { return Ok(tap); };
                let frame = SongLuaLiveFrame::from_gameplay(game, 0.0);
                feed_live_stats(&stage, &players, &frame)?;
                let music_rate = crate::song_tables::current_music_rate(&lua)?;
                let beat = song_timing.as_ref().map_or(frame.beat as f32,
                    |timing| timing.get_song_position_with_rate(frame.music_seconds as f32, music_rate).beat);
                crate::set_compile_song_runtime_values(&lua, beat, frame.music_seconds as f32)?;
                crate::lua_util::set_live_frame(&lua, frame.screen_seconds, f64::from(beat), 0.0);
                lua.set_app_data(LivePlayers(frame.players));
                let mut native_tap = tap;
                // Player.cpp sends mine hits dim even when Blind is active.
                native_tap.bright = tap.score != "HitMine" &&
                    (frame.players[tap.player].combo as u32 > bright_combo_threshold
                    || game.effective_visibility_effects_for_player(tap.player).blind != 0.0);
                invoke_note_field_tap_callback(&lua, field, &callback, native_tap, game.cols_per_player())
            })();
            let result = match result {
                Ok(tap) => tap,
                Err(error) => {
                    let _ = crate::lua_util::report_update_error(&lua, field,
                        "__songlua_did_tap_note_error_reported", "DidTapNote callback", &error);
                    tap
                }
            };
            let writes = lua.remove_app_data::<PlayerWrites>().map_or_else(Vec::new, |writes| writes.0);
            apply_player_writes(writes, game);
            result
        }));
        Ok(())
    }

    pub fn context(&self) -> &SongLuaCompileContext { &self.context }

    pub fn install_step_callback<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        &self, game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    ) {
        let lua = self.lua.clone();
        let stage = self.stage.clone();
        let players = self.players.clone();
        let song_timing = self.context.song_timing.clone();
        game.step_callback = Some(Box::new(move |game, column| {
            let result = (|| -> mlua::Result<()> {
                let frame = SongLuaLiveFrame::from_gameplay(game, 0.0);
                feed_live_stats(&stage, &players, &frame)?;
                let music_rate = crate::song_tables::current_music_rate(&lua)?;
                let beat = song_timing.as_ref().map_or(frame.beat as f32,
                    |timing| timing.get_song_position_with_rate(frame.music_seconds as f32, music_rate).beat);
                crate::set_compile_song_runtime_values(&lua, beat, frame.music_seconds as f32)?;
                crate::lua_util::set_live_frame(&lua, frame.screen_seconds, f64::from(beat), 0.0);
                lua.set_app_data(LivePlayers(frame.players));
                let player = game.player_for_col(column);
                let params = lua.create_table()?;
                params.raw_set("PlayerNumber", crate::player_number_name(player))?;
                params.raw_set("Column", column - player * game.cols_per_player())?;
                // PlayerState initializes MultiPlayer to Invalid. Enum::Push
                // exposes that as nil, so ordinary cabinet play has no key.
                crate::lua_util::broadcast_native_song_lua_message(&lua, "Step", Some(Value::Table(params)))?;
                crate::lua_util::broadcast_native_song_lua_message(&lua,
                    &format!("StepP{}", player + 1), Some(Value::Table(lua.create_table()?)))
            })();
            if let Err(error) = result
                && let Ok(screen) = lua.globals().raw_get::<Table>("__songlua_top_screen") {
                let _ = crate::lua_util::report_update_error(&lua, &screen,
                    "__songlua_step_error_reported", "StepMessage", &error);
            }
            let writes = lua.remove_app_data::<PlayerWrites>().map_or_else(Vec::new, |writes| writes.0);
            apply_player_writes(writes, game);
        }));
    }

    pub fn install_tap_row_judgment_callback<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        &self, game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    ) {
        let lua = self.lua.clone();
        let stage = self.stage.clone();
        let players = self.players.clone();
        let song_timing = self.context.song_timing.clone();
        game.tap_row_judgment_callback = Some(Box::new(move |game, player, row_entry, judgment| {
            let result = (|| -> mlua::Result<()> {
                let frame = SongLuaLiveFrame::from_gameplay(game, 0.0);
                feed_live_stats(&stage, &players, &frame)?;
                let music_rate = crate::song_tables::current_music_rate(&lua)?;
                let beat = song_timing.as_ref().map_or(frame.beat as f32,
                    |timing| timing.get_song_position_with_rate(frame.music_seconds as f32, music_rate).beat);
                crate::set_compile_song_runtime_values(&lua, beat, frame.music_seconds as f32)?;
                crate::lua_util::set_live_frame(&lua, frame.screen_seconds, f64::from(beat), 0.0);
                lua.set_app_data(LivePlayers(frame.players));
                let params = tap_row_judgment_params(&lua, game, player, row_entry, judgment)?;
                crate::lua_util::broadcast_native_song_lua_message(&lua, "Judgment", Some(Value::Table(params)))
            })();
            if let Err(error) = result
                && let Ok(screen) = lua.globals().raw_get::<Table>("__songlua_top_screen") {
                let _ = crate::lua_util::report_update_error(&lua, &screen,
                    "__songlua_judgment_error_reported", "JudgmentMessage", &error);
            }
            let writes = lua.remove_app_data::<PlayerWrites>().map_or_else(Vec::new, |writes| writes.0);
            apply_player_writes(writes, game);
        }));
    }

    pub fn install_hold_judgment_callback<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        &self, game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    ) {
        let lua = self.lua.clone();
        let stage = self.stage.clone();
        let players = self.players.clone();
        let song_timing = self.context.song_timing.clone();
        game.hold_judgment_callback = Some(Box::new(move |game, player, note_index, result| {
            let dispatched = (|| -> mlua::Result<()> {
                let frame = SongLuaLiveFrame::from_gameplay(game, 0.0);
                feed_live_stats(&stage, &players, &frame)?;
                let music_rate = crate::song_tables::current_music_rate(&lua)?;
                let beat = song_timing.as_ref().map_or(frame.beat as f32,
                    |timing| timing.get_song_position_with_rate(frame.music_seconds as f32, music_rate).beat);
                crate::set_compile_song_runtime_values(&lua, beat, frame.music_seconds as f32)?;
                crate::lua_util::set_live_frame(&lua, frame.screen_seconds, f64::from(beat), 0.0);
                lua.set_app_data(LivePlayers(frame.players));
                let note = &game.chart_runtime.notes[note_index];
                let params = lua.create_table()?;
                params.raw_set("Player", format!("PlayerNumber_P{}", player + 1))?;
                params.raw_set("FirstTrack", note.column - player * game.cols_per_player())?;
                params.raw_set("NumTracks", game.cols_per_player())?;
                params.raw_set("TapNoteScore", note.result.map_or("TapNoteScore_None", judgment_score))?;
                params.raw_set("HoldNoteScore", match result {
                    deadsync_rules::note::HoldResult::Held => "HoldNoteScore_Held",
                    deadsync_rules::note::HoldResult::LetGo => "HoldNoteScore_LetGo",
                    deadsync_rules::note::HoldResult::Missed => "HoldNoteScore_MissedHold",
                })?;
                params.raw_set("EarlyTapNoteScore", note.early_result.map_or("TapNoteScore_None", judgment_score))?;
                params.raw_set("TapNote", live_tap_note(&lua, &params, note)?)?;
                crate::lua_util::broadcast_native_song_lua_message(&lua, "Judgment", Some(Value::Table(params)))
            })();
            if let Err(error) = dispatched
                && let Ok(screen) = lua.globals().raw_get::<Table>("__songlua_top_screen") {
                let _ = crate::lua_util::report_update_error(&lua, &screen,
                    "__songlua_hold_judgment_error_reported", "Hold JudgmentMessage", &error);
            }
            let writes = lua.remove_app_data::<PlayerWrites>().map_or_else(Vec::new, |writes| writes.0);
            apply_player_writes(writes, game);
        }));
    }

    pub fn install_avoided_mine_judgment_callback<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        &self, game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    ) {
        let lua = self.lua.clone();
        let stage = self.stage.clone();
        let players = self.players.clone();
        let song_timing = self.context.song_timing.clone();
        game.avoided_mine_judgment_callback = Some(Box::new(move |game, player, column| {
            let dispatched = (|| -> mlua::Result<()> {
                let frame = SongLuaLiveFrame::from_gameplay(game, 0.0);
                feed_live_stats(&stage, &players, &frame)?;
                let music_rate = crate::song_tables::current_music_rate(&lua)?;
                let beat = song_timing.as_ref().map_or(frame.beat as f32,
                    |timing| timing.get_song_position_with_rate(frame.music_seconds as f32, music_rate).beat);
                crate::set_compile_song_runtime_values(&lua, beat, frame.music_seconds as f32)?;
                crate::lua_util::set_live_frame(&lua, frame.screen_seconds, f64::from(beat), 0.0);
                lua.set_app_data(LivePlayers(frame.players));
                let params = lua.create_table()?;
                params.raw_set("Player", crate::player_number_name(player))?;
                params.raw_set("TapNoteScore", "TapNoteScore_AvoidMine")?;
                params.raw_set("FirstTrack", column - player * game.cols_per_player())?;
                crate::lua_util::broadcast_native_song_lua_message(&lua, "Judgment", Some(Value::Table(params)))
            })();
            if let Err(error) = dispatched
                && let Ok(screen) = lua.globals().raw_get::<Table>("__songlua_top_screen") {
                let _ = crate::lua_util::report_update_error(&lua, &screen,
                    "__songlua_avoided_mine_error_reported", "AvoidMine JudgmentMessage", &error);
            }
            let writes = lua.remove_app_data::<PlayerWrites>().map_or_else(Vec::new, |writes| writes.0);
            apply_player_writes(writes, game);
        }));
    }

    /// ScreenGameplay checks completion after ActorFrame updates and Lua writes.
    pub fn note_gameplay_outro_after_actors(&mut self) {
        self.outro_started = true;
    }

    /// LeaveGameplay has dispatched Off; its out transition can now advance.
    pub fn outro_off_dispatched(&self) -> bool {
        self.outro_options_reset && !self.outro_off_pending
    }

    pub fn failed_outro(&self) -> bool {
        self.outro_failed
    }

    pub fn apply_pending_player_writes<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        &self, game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    ) {
        let writes = self.lua.remove_app_data::<PlayerWrites>().map_or_else(Vec::new, |writes| writes.0);
        apply_player_writes(writes, game);
    }

    /// GameState updates PlayerState options before ScreenGameplay updates
    /// players. FailType is copied immediately, without a numeric approach.
    /// Preserve the gameplay configuration when Lua has no explicit policy.
    pub fn prepare_gameplay_player_options<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        &self, game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    ) -> mlua::Result<()> {
        for player in 0..LUA_PLAYERS {
            game.mods.song_lua_fail_types[player] =
                crate::song_tables::live_player_fail_type(&self.lua, player, 2)?;
        }
        Ok(())
    }

    pub fn apply_output_player_writes<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        &self, output: &SongLuaLiveOutput, game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    ) {
        apply_player_writes(output.player_writes.iter().copied(), game);
    }

    /// Supply neutral native allocations for the next screen epoch during a
    /// replay. A reload installs them when constructing its replacement scene.
    #[cfg(feature = "test-support")]
    pub fn set_message_actor_order(&mut self, order: Vec<(String, usize)>) {
        self.context.message_actor_order = order;
    }

    /// GAMESTATE may select a different chart while the current Player still
    /// owns the note field loaded at screen entry.
    pub fn selected_context(&self) -> mlua::Result<SongLuaCompileContext> {
        crate::host::selected_scene_context(&self.lua, &self.context)
    }

    pub fn matches_entries(&self, paths: &[&std::path::Path], primary: usize) -> bool {
        primary == self.primary_index && paths.len() == self.entry_paths.len()
            && paths.iter().zip(&self.entry_paths).all(|(path, saved)|
                crate::entry_file_path(path).as_ref() == Some(saved))
    }

    /// ScreenManager pops the outgoing screen before dispatching End. The
    /// caller consumes its effects before deleting or replacing the scene.
    pub fn end(&mut self) -> mlua::Result<SongLuaLiveOutput> {
        if self.ended { return Ok(SongLuaLiveOutput::default()); }
        self.ended = true;
        if let Value::Table(root) = &self.root {
            crate::lua_util::end_actor_scene(&self.lua, root)?;
        }
        take_output(&self.lua)
    }

    /// Screen-entry binding from shared Lua actor positions to prepared visual
    /// slots. Ownership remains in this scene; the runtime only receives poses.
    pub fn bind_targets(&mut self, targets: Vec<(usize, SongLuaLiveTarget)>) -> Result<(), String> {
        if targets.iter().any(|(global, _)| *global >= self.states.len()) {
            return Err("live Lua binding index is outside its scene".into());
        }
        self.visual_targets = targets.iter().map(|&(global, target)|
            (self.actors[global].to_pointer() as usize, target)).collect();
        self.targets = targets;
        Ok(())
    }

    fn proxy_target(&self, owned: &Table, owner: SongLuaLiveTarget) -> Result<crate::SongLuaProxyTarget, String> {
        let mut target = crate::lua_util::read_proxy_target_kind(owned)?
            .unwrap_or(crate::SongLuaProxyTarget::Actor { overlay_index: usize::MAX });
        if let crate::SongLuaProxyTarget::Actor { overlay_index } = &mut target {
            // Lua stores actor identity; presentation uses an index within the
            // proxy's own layer. Resolve again after live On/Update/Input calls.
            *overlay_index = self.visual_targets.get(overlay_index).and_then(|target| {
                match (*target, owner) {
                    (SongLuaLiveTarget::Root(local), SongLuaLiveTarget::Root(_)) => Some(local),
                    (SongLuaLiveTarget::Background(layer, local), SongLuaLiveTarget::Background(owner, _))
                    | (SongLuaLiveTarget::Foreground(layer, local), SongLuaLiveTarget::Foreground(owner, _))
                        if layer == owner => Some(local),
                    _ => None,
                }
            }).unwrap_or(usize::MAX);
        }
        Ok(target)
    }

    /// Apply current local poses. The screen marks these actors dynamic in
    /// FrameScratch at entry, before rendering starts; baked timelines stay empty.
    pub fn write_visuals<N, M>(
        &self,
        visuals: &mut deadsync_gameplay::SongLuaRuntimeVisuals<
            crate::SongLuaOverlayActor<crate::SongLuaOverlayKind<N, M, deadlib_present::actors::TextAttribute>>, crate::SongLuaCapturedActor,
            crate::gameplay::SongLuaRuntimeOverlayStateDelta>,
    ) -> Result<(), String> {
        for &(global, target) in &self.targets {
            let actor = match target {
                SongLuaLiveTarget::Root(local) => visuals.overlays.get_mut(local),
                SongLuaLiveTarget::Background(layer, local) => visuals.background_visual_layers.get_mut(layer)
                    .and_then(|layer| layer.overlays.get_mut(local)),
                SongLuaLiveTarget::Foreground(layer, local) => visuals.foreground_visual_layers.get_mut(layer)
                    .and_then(|layer| layer.overlays.get_mut(local)),
            }.ok_or_else(|| "live Lua actor has no prepared visual slot".to_owned())?;
            actor.initial_state = self.states[global];
            let owned = &self.actors[global];
            sync_visual_content(owned, &mut actor.kind)?;
            if let crate::SongLuaOverlayKind::ActorProxy { target: proxy_target } = &mut actor.kind {
                *proxy_target = self.proxy_target(owned, target)?;
            }
        }
        for (target, state) in self.tracked_states() {
            use crate::SongLuaTrackedActorTarget as T;
            match target {
                T::Player(p) => visuals.player_actors[p].initial_state = state,
                T::PlayerJudgment(p) => visuals.player_actors[p].judgment.initial_state = state,
                T::PlayerCombo(p) => visuals.player_actors[p].combo.initial_state = state,
                T::SongForeground => visuals.song_foreground.initial_state = state,
                T::ScreenLayer(layer) => visuals.screen_layers[layer].initial_state = state,
            }
        }
        visuals.hidden_players = std::array::from_fn(|p| !visuals.player_actors[p].initial_state.visible);
        Ok(())
    }

    /// Observe factory identities and production visual bindings without
    /// importing reference poses, vertices, or callback results.
    #[cfg(feature = "test-support")]
    pub fn visual_bindings(&self) -> mlua::Result<Vec<(String, SongLuaLiveTarget)>> {
        self.targets.iter().map(|&(global, target)|
            Ok((crate::lua_util::message_actor_path(&self.actors[global])?, target))).collect()
    }

    #[cfg(feature = "test-support")]
    pub fn visual_draw_bindings(&self) -> &[(usize, SongLuaLiveTarget)] { &self.targets }

    pub fn states(&self) -> &[SongLuaOverlayState] { &self.states }

    /// Screen::GetNextScreenName reads the stored override when the transition
    /// completes. An empty override leaves the theme's default destination.
    pub fn next_screen_name(&self) -> mlua::Result<Option<String>> {
        let screen: Table = self.lua.globals().raw_get("__songlua_top_screen")?;
        Ok(screen.raw_get::<Option<String>>("__songlua_next_screen_name")?
            .filter(|name| !name.is_empty()))
    }

    pub fn tracked_states(&self) -> impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)> + '_ {
        self.tracked.iter().map(|(target, _, state)| (*target, *state))
    }

    pub fn take_startup_output(&mut self) -> SongLuaLiveOutput {
        std::mem::take(&mut self.startup_output)
    }
}

fn sync_visual_content<N, M>(owned: &Table, kind: &mut crate::SongLuaOverlayKind<N, M, deadlib_present::actors::TextAttribute>) -> Result<(), String> {
    match kind {
        crate::SongLuaOverlayKind::ActorMultiVertex { vertices, texture_path, texture_key, .. } => {
            *vertices = crate::lua_util::read_actor_multi_vertex_mesh(owned)?.unwrap_or_else(|| std::sync::Arc::from([]));
            let path = crate::lua_util::live_multi_vertex_texture_path(owned).map_err(|error| error.to_string())?;
            if *texture_path != path {
                *texture_key = path.as_ref().map(|path| std::sync::Arc::from(path.to_string_lossy().as_ref()));
                *texture_path = path;
            }
        }
        crate::SongLuaOverlayKind::BitmapText { text, attributes, stroke_color, .. } => {
            let current = crate::lua_util::lua_text_value(owned.get("Text").map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            if text.as_ref() != current { *text = current.into(); }
            *attributes = crate::lua_util::read_bitmap_text_attributes(owned)?;
            *stroke_color = crate::lua_util::read_actor_color_field(owned, "__songlua_stroke_color")?;
        }
        crate::SongLuaOverlayKind::Sprite { texture_path, texture_key, states, .. } => {
            if let Some(path) = crate::lua_util::actor_texture_path(owned).map_err(|error| error.to_string())?
                && *texture_path != path
            {
                *texture_key = std::sync::Arc::from(path.to_string_lossy().as_ref());
                *texture_path = path;
            }
            *states = crate::lua_util::read_sprite_states(owned)?.into();
        }
        _ => {}
    }
    Ok(())
}

fn take_output(lua: &Lua) -> mlua::Result<SongLuaLiveOutput> {
    let request = if let Some(request) = lua.remove_app_data::<crate::host::SongLuaScreenRequest>() {
        Some(SongLuaLiveRequest::NewScreen(request.0))
    } else {
        lua.remove_app_data::<TransitionRequest>().map(|request| SongLuaLiveRequest::Transition(request.0))
    };
    let broadcasts = crate::lua_util::take_runtime_broadcasts(lua);
    let source_broadcasts = crate::lua_util::take_source_broadcasts(lua);
    let source_requests = lua.app_data_mut::<SourceRequests>()
        .map_or_else(Vec::new, |mut events| std::mem::take(&mut events.0));
    let sounds = crate::lua_util::take_runtime_sounds(lua);
    let errors = lua.remove_app_data::<crate::lua_util::SongLuaUpdateErrors>()
        .map_or_else(Vec::new, |errors| errors.0);
    let coverage_gaps = crate::lua_util::take_message_coverage(lua);
    if let Some(history) = lua.globals().raw_get::<Option<Table>>(crate::SONG_LUA_BROADCASTS_KEY)? {
        history.clear()?;
    }
    let player_writes = lua.remove_app_data::<PlayerWrites>().map_or_else(Vec::new, |writes| writes.0);
    let draw_frames = crate::draw_capture::take_live(lua).map_err(mlua::Error::external)?;
    Ok(SongLuaLiveOutput { request, source_requests, broadcasts, source_broadcasts, sounds, errors, coverage_gaps, player_writes, draw_frames })
}

pub(crate) fn read_state(lua: &Lua, actor: &Table) -> mlua::Result<SongLuaOverlayState> {
    let mut state = crate::lua_util::actor_overlay_state_batched(lua, actor).map_err(mlua::Error::external)?;
    crate::lua_util::replay_tween_pose(lua, actor, &mut state);
    state.spin_baked = crate::lua_util::spin_render_pose(actor)?.is_some();
    Ok(state)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "test-support")]
    #[test]
    fn live_overlay_stage_observation_precedes_foreground_mutations() {
        let mut compiled = compile_with_context("overlay-stage-phase", r#"
return Def.Actor {
    InitCommand = function(self) self:sleep(1000) end,
    OnCommand = function(self)
        GAMESTATE:SetCurrentSteps(PLAYER_1, GAMESTATE:GetCurrentSong():GetAllSteps()[2])
        SCREENMAN:GetTopScreen():GetChild('PlayerP1'):SetLife(0.2)
        STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1):FailPlayer()
    end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![-24.0];
            context.players[0].enabled = true;
            context.players[1].enabled = false;
            context.song_steps = [SongLuaDifficulty::Challenge, SongLuaDifficulty::Edit]
                .into_iter().map(|difficulty| crate::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(), difficulty,
                    description: format!("{difficulty:?}"), chart_name: String::new(),
                    credit: String::new(), meter: 9, music_file: String::new(), possible_grade_points: 100,
                }).collect();
            context.player_step_indices = [Some(0), Some(0)];
        });
        let mut scene = compiled.live_scene.take().unwrap();
        scene.enable_overlay_stage_observer().unwrap();
        let mut frame = SongLuaLiveFrame { beat: -4.0, music_seconds: -2.0,
            screen_seconds: 0.0, delta_seconds: 0.0, ..Default::default() };
        frame.players[0] = SongLuaLivePlayer { possible_points: 100, actual_points: 17,
            current_possible_points: 25, life: 0.7, fail_enabled: true, ..Default::default() };
        frame.players[0].tap_scores[4] = 3;
        frame.players[0].tap_scores[8] = 1;
        frame.players[0].tap_scores[9] = 2;
        let output = scene.advance(frame).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(scene.selected_context().unwrap().player_step_indices[0], Some(1));
        assert!(output.player_writes.contains(&SongLuaLivePlayerWrite::Fail { player: 0 }));
        let observed = scene.take_overlay_stage_observation()
            .expect("overlay stage observation must precede foreground mutations");
        assert_eq!(observed.player_step_indices[0], Some(0));
        assert_eq!(observed.music_seconds, -2.0);
        assert_eq!(observed.beat, -4.0);
        let player = observed.players[0];
        assert_eq!((player.possible_points, player.actual_points, player.current_possible_points), (100, 17, 25));
        assert_eq!(player.life, 0.7);
        assert!(!player.failed);
        assert_eq!((player.tap_scores[4], player.tap_scores[8], player.tap_scores[9]), (3, 1, 2));
        assert!(scene.take_overlay_stage_observation().is_none(), "observations drain once");
    }

    #[test]
    fn live_untyped_foreground_uses_native_dummy_actor() {
        let mut compiled = compile_with_context("untyped-foreground", "CONSTRUCTED = true; return {}", |context| {
            context.layer_start_beats = vec![0.0];
            context.foreground_names = vec!["fallback".into()];
        });
        let mut scene = compiled.live_scene.take().expect("native dummy foreground");
        let output = scene.advance(SongLuaLiveFrame { beat: 0.1, music_seconds: 0.1,
            screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene.lua.load(r#"
assert(CONSTRUCTED, 'construction side effects survive the fallback')
local foreground = SCREENMAN:GetTopScreen():GetChild('SongForeground'):GetChild('fallback')
assert(foreground and foreground:GetName() == 'fallback')
assert(foreground:GetParent():GetName() == 'SongForeground')
foreground:visible(true)
assert(foreground:GetVisible())
"#).exec().expect("foreground owns a real dummy Actor");
    }

    #[test]
    fn live_untyped_declaration_discards_fields_commands_and_children() {
        let mut compiled = compile_with_context("untyped-declaration", r#"
local untyped = {
    Name = 'Ignored',
    InitCommand = function() error('untyped Init must not run') end,
    OnCommand = function() error('untyped On must not run') end,
    Def.Actor { InitCommand = function() error('untyped children must not load') end },
}
return Def.ActorFrame {
    InitCommand = function(self) ROOT = self end,
    untyped,
}
"#, |_| {});
        let mut scene = compiled.live_scene.take().expect("native dummy child");
        let output = scene.advance(SongLuaLiveFrame { beat: 0.1, music_seconds: 0.1,
            screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene.lua.load(r#"
assert(ROOT:GetChild('Ignored') == nil, 'invalid declarations do not load Name')
local child = ROOT:GetChild('')
assert(child and child:GetName() == '')
child:visible(false)
assert(not child:GetVisible())
"#).exec().expect("invalid declarations load only the native dummy Actor");
    }

    #[test]
    fn live_arrow_alpha_uses_visibility_and_native_far_edge_fade() {
        let mut compiled = compile_with_context("native-arrow-alpha", "return Def.Actor {InitCommand = function(self) ALPHA_ACTOR = self end}",
            |context| context.defer_initial_live_update = true);
        let mut scene = compiled.live_scene.take().unwrap();
        scene.advance(SongLuaLiveFrame {
            music_seconds: 2.0, screen_seconds: 2.0, beat: 4.0,
            delta_seconds: 0.01, ..Default::default()
        }).unwrap();
        scene.lua.load(r#"
local ps = GAMESTATE:GetPlayerState(PLAYER_1)
local o = ps:GetCurrentPlayerOptions()
local alpha = ArrowEffects.GetAlpha
assert(alpha(ps, 1, 720) == 1)
assert(alpha(ps, 1, 721) == -math.huge, 'native far-edge fade retains negative infinity')
assert(math.abs(alpha(ps, 1, 600, nil, nil, 1000, 0.5) - 0.8) < 0.000001)
assert(alpha(ps, 1, 1000, nil, nil, 1000, 0.5) == 0)
assert(alpha(ps, 1, 1250, nil, nil, 1000, 0.5) == -0.5)
assert(alpha(ps, 1, 100, 0.5) == 0 and alpha(ps, 1, 100, 0.499) == 1)
o:Reverse(1)
assert(alpha(ps, 1, 721) == -math.huge, 'alpha uses position without reverse')
o:Stealth(0.6)
assert(alpha(ps, 1, 100) == 0)
assert(alpha(ps, 1, -1) == 1, 'past receptors remain visible by default')
o:StealthPastReceptors(true)
assert(alpha(ps, 1, -1) == 0)
assert(alpha(ps, 1, 100, 0) == 1, 'fail fade overrides appearance visibility')
o:Stealth(0)
o:Stealth1(0.6)
assert(alpha(ps, 1, 100) == 0 and alpha(ps, 2, 100) == 1)
o:Stealth1(0)
o:Hidden(1)
assert(alpha(ps, 1, 100) == 0 and alpha(ps, 1, 161) == 1)
o:Hidden(0)
o:Sudden(1)
assert(alpha(ps, 1, 201) == 0 and alpha(ps, 1, 159) == 1)
o:Sudden(0)
o:Mini(1)
o:Hidden(1)
assert(alpha(ps, 1, 250) == 0 and alpha(ps, 1, 321) == 1,
    'mini scales the appearance center line')
local actor = ALPHA_ACTOR
actor:diffusealpha(alpha(ps, 1, 721))
assert(actor:GetDiffuseAlpha() == -math.huge, 'the raw alpha reaches the real actor')
"#).exec().expect("native alpha visibility and far-edge fade");
    }

    #[test]
    fn live_game_state_class_methods_dispatch_on_owned_instance() {
        let mut compiled = compile_with_context("game-state-class-extension", r#"
assert(type(GameState) == 'table', 'GameState class methods must be available')
assert(GameState ~= GAMESTATE, 'class methods and owned instance are distinct')
function GameState:ApplyModifiers(level)
    assert(self == GAMESTATE)
    self:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song'):Mini(level)
    return self
end
return Def.Actor {}
"#, |context| context.defer_initial_live_update = true);
        let mut scene = compiled.live_scene.take().expect("owned gameplay scene");
        scene.advance(SongLuaLiveFrame {
            music_seconds: 2.0, screen_seconds: 2.0, beat: 4.0,
            delta_seconds: 0.01, ..Default::default()
        }).unwrap();
        scene.lua.set_app_data(QueryTrace::default());
        let game = scene.lua.globals().get::<Table>("GAMESTATE").unwrap();
        let class = scene.lua.globals().get::<Table>("GameState").unwrap();
        let native = class.get::<Function>("GetSongBeat").unwrap();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        class.set("GetSongBeat", scene.lua.create_function({
            let calls = calls.clone();
            move |_, args: MultiValue| {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                native.call::<MultiValue>(args)
            }
        }).unwrap()).unwrap();
        hook_query(&scene.lua, &game, "GameState", "GetSongBeat").unwrap();
        hook_query(&scene.lua, &game, "GameState", "GetSongBeat").unwrap();
        scene.lua.load(r#"
local nativeBeat = GameState.GetSongBeat
function GameState:GetSongBeat()
    assert(self == GAMESTATE)
    return nativeBeat(self) + 0.125
end
assert(GAMESTATE:GetSongBeat() == 4.125)
assert(GAMESTATE:ApplyModifiers(0.25) == GAMESTATE)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song'):Mini() == 0.25)
assert(GameState.GetSongBeat(GAMESTATE) == 4.125)
"#).exec().expect("class extension and override through the instance");
        let trace = scene.lua.app_data_ref::<QueryTrace>().unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 2,
            "hooking must not manufacture extra native getter calls");
        assert_eq!(trace.hooks.len(), 1, "one hook per native class method");
        assert_eq!(trace.hooks[0].0.to_pointer(), class.to_pointer(),
            "native hooks preserve class override lookup through the instance");
        assert!(matches!(game.raw_get::<Value>("GetSongBeat").unwrap(), Value::Nil),
            "a recorder wrapper must not shadow the authored class override");
    }

    #[test]
    fn live_screen_request_calls_preserve_duplicates_and_final_target() {
        let compiled = compile_with_context("screen-request-calls", "return Def.Actor{}", |_| {});
        let scene = compiled.live_scene.expect("live request observations");
        crate::set_compile_song_runtime_values(&scene.lua, 2.0, 1.25).unwrap();
        scene.lua.load(r#"
SCREENMAN:SetNewScreen('ScreenSelectMusic')
SCREENMAN:SetNewScreen('ScreenGameplay')
SCREENMAN:SetNewScreen('ScreenGameplay')
"#).exec().unwrap();
        let mut output = take_output(&scene.lua).unwrap();
        assert_eq!(output.request, Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into())));
        assert_eq!(output.source_requests.len(), 3, "every authored screen request remains observable");
        for (event, name) in output.source_requests.iter().zip(["ScreenSelectMusic", "ScreenGameplay", "ScreenGameplay"]) {
            assert_eq!(event.music_seconds, 1.25);
            assert_eq!(event.request, SongLuaLiveRequest::NewScreen(name.into()));
        }
        assert!(take_output(&scene.lua).unwrap().source_requests.is_empty(), "observations drain once");
        scene.lua.load("SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_DoNextScreen')").exec().unwrap();
        let transition = take_output(&scene.lua).unwrap();
        assert_eq!(transition.source_requests.len(), 1);
        assert_eq!(transition.source_requests[0].request, SongLuaLiveRequest::Transition("SM_DoNextScreen".into()));
        output.append(transition);
        assert_eq!(output.source_requests.len(), 4, "output merging preserves all source calls");
        assert_eq!(output.request, Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into())));
    }

    #[test]
    fn live_gameplay_reload_copies_human_steps_to_disabled_player() {
        let mut compiled = compile_with_context("reload-cpu-steps", r#"
SCENES = (SCENES or 0) + 1
local scene = SCENES
local p1 = GAMESTATE:GetCurrentSteps(PLAYER_1)
local p2 = GAMESTATE:GetCurrentSteps(PLAYER_2)
assert(GAMESTATE:IsPlayerEnabled(PLAYER_1) and not GAMESTATE:IsPlayerEnabled(PLAYER_2))
if scene == 1 then
    OLD_CPU_STEPS = p2
else
    assert(p1 == CHOSEN_STEPS and p2 == CHOSEN_STEPS,
        'native gameplay Init copies human Steps to the nonhuman player')
    assert(p2:GetDifficulty() == 'Difficulty_Edit')
end
return Def.Actor {
    OnCommand = function(self)
        if scene == 1 then
            CHOSEN_STEPS = GAMESTATE:GetCurrentSong():GetAllSteps()[2]
            GAMESTATE:SetCurrentSteps(PLAYER_1, CHOSEN_STEPS)
            assert(GAMESTATE:GetCurrentSteps(PLAYER_2) == OLD_CPU_STEPS,
                'SetCurrentSteps only forces enabled other players')
            SCREENMAN:SetNewScreen('ScreenGameplay')
        end
    end,
    EndCommand = function(self)
        if scene == 1 then
            END_SAW_OLD_CPU = GAMESTATE:GetCurrentSteps(PLAYER_2) == OLD_CPU_STEPS
        end
    end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].enabled = true;
            context.players[1].enabled = false;
            context.song_steps = [SongLuaDifficulty::Challenge, SongLuaDifficulty::Edit]
                .into_iter().map(|difficulty| crate::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(), difficulty,
                    description: format!("{difficulty:?}"), chart_name: String::new(),
                    credit: String::new(), meter: 9, music_file: String::new(), possible_grade_points: 0,
                }).collect();
            context.player_step_indices = [Some(0), Some(0)];
        });
        let mut scene = compiled.live_scene.take().expect("first owned gameplay scene");
        let output = scene.advance(SongLuaLiveFrame::default()).expect("first On selects Edit");
        assert_eq!(output.request, Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into())));
        let mut layers = crate::reload_song_lua_with_default_host(scene,
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None)
            .expect("native gameplay Init copies human Steps to the nonhuman player");
        let mut scene = layers[0].live_scene.take().expect("replacement gameplay scene");
        assert_eq!(scene.lua.globals().get::<u32>("SCENES").unwrap(), 2);
        assert!(scene.lua.globals().get::<bool>("END_SAW_OLD_CPU").unwrap(),
            "outgoing End runs before the new gameplay initialization");
        assert_eq!(scene.selected_context().unwrap().player_step_indices, [Some(1), Some(1)]);
        let output = scene.advance(SongLuaLiveFrame::default()).expect("replacement On");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(output.request, None, "Edit construction does not replay the introduction");
    }

    #[test]
    fn live_numeric_song_selectors_retain_chart_across_reload() {
        let mut compiled = compile_with_context("numeric-song-selector-reload", r#"
NUMERIC_SCENES = (NUMERIC_SCENES or 0) + 1
local scene = NUMERIC_SCENES
local names = {'Beginner','Easy','Medium','Hard','Challenge','Edit'}
return Def.Actor { OnCommand = function(self)
    local song = GAMESTATE:GetCurrentSong()
    for code = 0, 5 do
        local step = song:GetOneSteps(0, code)
        assert(step ~= nil, 'numeric GetOneSteps must select a real chart')
        assert(step:GetDifficulty() == 'Difficulty_' .. names[code + 1])
    end
    assert(#song:GetStepsByStepsType(0) == 6 and song:HasStepsType(0))
    assert(song:HasStepsTypeAndDifficulty(0, 2) and song:HasEdits(0))
    assert(song:GetOneSteps(6, 2):GetStepsType() == 'StepsType_Pump_Single')
    if scene == 1 then
        NUMERIC_CHOSEN = song:GetOneSteps(0.0, 2.0)
        GAMESTATE:SetCurrentSteps(0, NUMERIC_CHOSEN)
        assert(GAMESTATE:GetCurrentSteps(0) == NUMERIC_CHOSEN)
        SCREENMAN:SetNewScreen('ScreenGameplay')
    else
        assert(GAMESTATE:GetCurrentSteps(0):GetDifficulty() == 'Difficulty_Medium')
        assert(GAMESTATE:GetCurrentSteps(1):GetDifficulty() == 'Difficulty_Medium')
    end
    self:sleep(9999)
end }
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].enabled = true;
            context.players[1].enabled = false;
            context.song_steps = [SongLuaDifficulty::Beginner, SongLuaDifficulty::Easy,
                SongLuaDifficulty::Medium, SongLuaDifficulty::Hard, SongLuaDifficulty::Challenge,
                SongLuaDifficulty::Edit, SongLuaDifficulty::Medium].into_iter().enumerate()
                .map(|(index, difficulty)| crate::SongLuaStep {
                    steps_type: if index == 6 { "StepsType_Pump_Single" } else { "StepsType_Dance_Single" }.into(),
                    difficulty, description: format!("{difficulty:?}"), chart_name: String::new(),
                    credit: String::new(), meter: 9, music_file: String::new(), possible_grade_points: 0,
                }).collect();
            context.player_step_indices = [Some(4), Some(4)];
        });
        let mut scene = compiled.live_scene.take().expect("initial numeric selector scene");
        let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
        assert!(output.errors.is_empty(), "numeric selector callback: {:?}", output.errors);
        assert_eq!(output.request, Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into())));
        assert_eq!(scene.selected_context().unwrap().player_step_indices, [Some(2), Some(4)]);
        let mut layers = crate::reload_song_lua_with_default_host(scene,
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).unwrap();
        let mut replacement = layers[0].live_scene.take().unwrap();
        assert_eq!(replacement.selected_context().unwrap().player_step_indices, [Some(2), Some(2)]);
        let output = replacement.advance(SongLuaLiveFrame::default()).unwrap();
        assert!(output.errors.is_empty(), "replacement callback: {:?}", output.errors);
        assert_eq!(output.request, None);
    }

    #[test]
    fn live_versus_children_retain_native_player_construction_groups() {
        let mut compiled = compile_with_context("versus-screen-child-order", r#"
return Def.Actor {
 OnCommand=function(self)
  local selected = {PlayerP1=true, PlayerP2=true, OniGameOverP1=true, OniGameOverP2=true}
  local names = {}
  SCREENMAN:GetTopScreen():RunCommandsOnChildren(function(child)
   local name=child:GetName()
   if selected[name] then names[#names+1]=name end
  end)
  assert(table.concat(names, ',') == 'PlayerP1,PlayerP2,OniGameOverP1,OniGameOverP2',
   'native Versus child traversal: '..table.concat(names, ','))
 end
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].enabled = true;
            context.players[1].enabled = true;
        });
        let mut scene = compiled.live_scene.take().expect("owned Versus screen");
        let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }

    #[test]
    fn live_gameplay_children_retain_native_owned_inventory() {
        let mut compiled = compile_with_context("native-screen-children", r#"
return Def.Actor {
    OnCommand=function(self)
        SCREEN = SCREENMAN:GetTopScreen()
        CHILDREN = SCREEN:GetChildren()
        CHILD_KEYS = {}
        for name, child in pairs(CHILDREN) do CHILD_KEYS[#CHILD_KEYS+1] = name end
    end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].enabled = true;
            context.players[1].enabled = false;
        });
        let mut scene = compiled.live_scene.take().expect("owned gameplay screen");
        let output = scene.advance(SongLuaLiveFrame::default()).expect("first foreground On");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let keys = scene.lua.globals().get::<Table>("CHILD_KEYS").unwrap();
        assert_eq!(keys.raw_len(), 22,
            "native gameplay child collection includes engine and adopted decoration owners");
        scene.lua.load(r#"
local expected = {'', 'ActiveAttackListP1', 'ActiveAttackListP2', 'Cancel', 'Debug',
    'Footer', 'Header', 'In', 'LifeP1', 'LyricDisplay', 'OniGameOverP1', 'Out',
    'Overlay', 'PlayerOptionsP1', 'PlayerP1', 'ScoreP1', 'SongBackground',
    'SongForeground', 'SongOptions', 'StepsDescriptionP1', 'StepsDisplayP1', 'Underlay'}
local nextChildren = SCREEN:GetChildren()
for _, name in ipairs(expected) do
    assert(CHILDREN[name] ~= nil, 'missing native owned child: ' .. name)
    if name ~= '' then
        assert(nextChildren[name] == CHILDREN[name] and SCREEN:GetChild(name) == CHILDREN[name],
            'named child lookup preserves owned actor identity: ' .. name)
        assert(CHILDREN[name]:GetParent() == SCREEN)
    end
end
assert(#CHILDREN[''] == 5, 'native duplicate empty names retain all five screen actors')
for i, child in ipairs(CHILDREN['']) do
    assert(nextChildren[''][i] == child and child:GetParent() == SCREEN)
end
assert(SCREEN:GetNumChildren() == 26, 'native screen counts owned actors, not table keys')
local relevant = {}
for _, name in ipairs(CHILD_KEYS) do
    if name == 'In' or name == 'Underlay' then relevant[#relevant+1] = name end
end
assert(relevant[1] == 'In' and relevant[2] == 'Underlay',
    'native child table traversal visits In before the Underlay score loop')
"#).exec().expect("native inventory, grouping, identity and traversal");
    }

    #[test]
    fn live_get_all_steps_returns_native_array_without_internal_getter_calls() {
        let compiled = compile_with_context("all-steps-native-array", "return Def.Actor{}", |context| {
            context.song_steps = [SongLuaDifficulty::Challenge, SongLuaDifficulty::Edit]
                .into_iter().enumerate().map(|(index, difficulty)| crate::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(), difficulty,
                    description: format!("chart {index}"), chart_name: String::new(),
                    credit: String::new(), meter: 9, music_file: String::new(), possible_grade_points: 0,
                }).collect();
            context.player_step_indices = [Some(0), Some(0)];
        });
        let scene = compiled.live_scene.expect("owned song array");
        let game = scene.lua.globals().get::<Table>("GAMESTATE").unwrap();
        let song = game.get::<Function>("GetCurrentSong").unwrap().call::<Table>(game).unwrap();
        let original = song.raw_get::<Table>("__songlua_steps").unwrap();
        scene.lua.globals().set("SONG", song).unwrap();
        scene.lua.globals().set("ORIGINAL", original).unwrap();
        scene.lua.load(r#"
GETTER_CALLS = 0
for _, step in ipairs(ORIGINAL) do
    for _, name in ipairs({'GetStepsType', 'GetDifficulty'}) do
        local getter = step[name]
        step[name] = function(self)
            GETTER_CALLS = GETTER_CALLS + 1
            return getter(self)
        end
    end
end
FIRST = SONG:GetAllSteps()
SECOND = SONG:GetAllSteps()
assert(SONG:GetOneSteps(0, 4) == ORIGINAL[1])
assert(SONG:GetOneSteps('StepsType_Dance_Single', 'Difficulty_Edit') == ORIGINAL[2])
assert(SONG:GetStepsByStepsType(0)[2] == ORIGINAL[2])
assert(SONG:HasStepsType(0))
assert(SONG:HasStepsTypeAndDifficulty(0, 5))
assert(SONG:HasEdits(0))
assert(not SONG:HasStepsType(1))
assert(SONG:GetOneSteps(0, 2) == nil)
"#).exec().unwrap();
        let globals = scene.lua.globals();
        assert_eq!(globals.get::<u32>("GETTER_CALLS").unwrap(), 0,
            "native SongUtil selectors read metadata without Lua getter callbacks");
        let first = globals.get::<Table>("FIRST").unwrap();
        let second = globals.get::<Table>("SECOND").unwrap();
        let original = globals.get::<Table>("ORIGINAL").unwrap();
        assert_eq!(first.raw_len(), 2);
        assert_eq!(second.raw_len(), 2);
        assert_ne!(first.to_pointer(), second.to_pointer(), "each getter returns a fresh array");
        assert_ne!(first.to_pointer(), original.to_pointer());
        for index in 1..=2 {
            assert_eq!(first.raw_get::<Table>(index).unwrap().to_pointer(), original.raw_get::<Table>(index).unwrap().to_pointer());
            assert_eq!(second.raw_get::<Table>(index).unwrap().to_pointer(), original.raw_get::<Table>(index).unwrap().to_pointer());
        }
        assert_eq!(first.raw_get::<Table>(2).unwrap().get::<Function>("GetDifficulty").unwrap()
            .call::<String>(first.raw_get::<Table>(2).unwrap()).unwrap(), "Difficulty_Edit");
        assert_eq!(globals.get::<u32>("GETTER_CALLS").unwrap(), 1,
            "authored getter calls remain observable");
    }

    #[test]
    fn live_infinite_sleep_keeps_foreground_sibling_reload_queue_active() {
        let mut compiled = compile_with_context("infinite-foreground-reload", r#"
local root
return Def.ActorFrame {
    InitCommand = function(self) root = self; FOREGROUND = self end,
    Def.Actor {
        InitCommand = function(self) PERSISTENT = self end,
        OnCommand = function(self) self:sleep(math.huge) end,
    },
    Def.Actor {
        OnCommand = function(self) self:queuecommand('WaitToAct') end,
        WaitToActCommand = function(self)
            if GAMESTATE:GetSongBeat() > 19 then
                SCREENMAN:SetNewScreen('ScreenGameplay')
            else
                self:sleep(0.01):queuecommand('WaitToAct')
            end
        end,
    },
}
"#, |context| {
            context.song_display_bpms = [120.0; 2];
            context.song_timing_bpms = vec![(0.0, 120.0)];
            context.layer_start_beats = vec![0.0];
            context.foreground_names = vec!["foreground".into()];
            context.defer_initial_live_update = true;
        });
        let mut scene = compiled.live_scene.take().expect("owned infinite foreground");
        assert!(scene.take_startup_output().errors.is_empty());
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.1, beat: 0.2,
            screen_seconds: 6.1, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let persistent = scene.lua.globals().get::<Table>("PERSISTENT").unwrap();
        let foreground = scene.lua.globals().get::<Table>("FOREGROUND").unwrap();
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &persistent).unwrap(), f32::INFINITY,
            "native infinite sleep retains foreground lifetime");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &foreground).unwrap(), f32::INFINITY);
        assert!(output.request.is_none());
        let mut requested = false;
        for index in 2..=100 {
            let music_seconds = index as f64 * 0.1;
            let output = scene.advance(SongLuaLiveFrame { music_seconds, beat: music_seconds * 2.0,
                screen_seconds: 6.0 + music_seconds, delta_seconds: 0.1,
                ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &persistent).unwrap(), f32::INFINITY);
            if let Some(request) = output.request {
                assert!(music_seconds * 2.0 > 19.0);
                assert!(matches!(request, SongLuaLiveRequest::NewScreen(ref name) if name == "ScreenGameplay"));
                requested = true;
                break;
            }
        }
        assert!(requested, "finite sibling queue reaches the authored reload beat beside an infinite sleep");
    }

    #[test]
    fn live_song_clock_and_timing_queries_follow_current_music_rate() {
        let mut compiled = compile_with_context("current-rate-clock", r#"
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        local timing = GAMESTATE:GetCurrentSong():GetTimingData()
        CLOCK = {GAMESTATE:GetSongBeat(),
            GAMESTATE:GetSongPosition():GetSongBeat(),
            timing:GetBeatFromElapsedTime(1), timing:GetElapsedTimeFromBeat(2)}
    end)
end }
"#, |context| {
            context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                0.0, 0.04, &deadsync_rules::timing::TimingSegments {
                    bpms: vec![(0.0, 120.0)], ..Default::default()
                }, &[]));
        });
        let mut scene = compiled.live_scene.take().unwrap();
        for rate in [1.0_f32, 1.25, 0.75, 1.0] {
            scene.lua.load(format!("GAMESTATE:ApplyGameCommand('mod,{rate}xmusic')"))
                .exec().unwrap();
            let output = scene.advance(SongLuaLiveFrame { music_seconds: 1.0,
                screen_seconds: 1.0, delta_seconds: 0.01, ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let clock: Table = scene.lua.globals().get("CLOCK").unwrap();
            // TimingData.cpp adds rate*globalOffset before the 120 BPM
            // traversal; its inverse subtracts it after the traversal.
            let beat = (1.0_f32 + rate * 0.04) * 2.0;
            for index in 1..=3 {
                assert_eq!(clock.get::<f32>(index).unwrap().to_bits(), beat.to_bits());
            }
            assert_eq!(clock.get::<f32>(4).unwrap().to_bits(),
                (1.0_f32 - rate * 0.04).to_bits());
        }
        scene.lua.load(r#"
assert(GAMESTATE:ApplyGameCommand('stagemod,0.75xmusic;mod,1.25xmusic') == GAMESTATE)
for _, level in ipairs({'Preferred','Stage','Song','Current'}) do
    assert(GAMESTATE:GetSongOptionsObject('ModsLevel_'..level):MusicRate() == 1.25)
end
GAMESTATE:ApplyGameCommand('mod,1.25xmusic;stagemod,0.75xmusic')
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Preferred'):MusicRate() == 1.25)
for _, level in ipairs({'Stage','Song','Current'}) do
    assert(GAMESTATE:GetSongOptionsObject('ModsLevel_'..level):MusicRate() == 0.75)
end
GAMESTATE:ApplyGameCommand('mod,50% drunk', 1)
for _, level in ipairs({'Preferred','Stage','Song','Current'}) do
    assert(GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_'..level):Drunk() == 0.5)
    assert(GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_'..level):Drunk() == 0)
end
"#).exec().unwrap();
    }

    #[test]
    fn live_initial_song_bps_uses_native_reset_before_timing_update() {
        for timing_clock in [false, true] {
            let mut compiled = compile_with_context(&format!("reset-bps-{timing_clock}"), r#"
INIT_CLOCKS = {}
local function init(self)
    INIT_CLOCKS[#INIT_CLOCKS + 1] = {GAMESTATE:GetSongBPS(),
        GAMESTATE:GetSongPosition():GetCurBPS(), GAMESTATE:GetSongBeat(),
        GAMESTATE:GetCurMusicSeconds()}
    self:effectperiod(4.0 / GAMESTATE:GetSongBPS())
end
return Def.ActorFrame {
    Def.Sprite {InitCommand = init}, Def.Sprite {InitCommand = init},
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            CLOCK = {GAMESTATE:GetSongBPS(), GAMESTATE:GetSongPosition():GetCurBPS()}
        end)
    end,
}
"#, |context| {
                context.defer_initial_live_update = true;
                context.song_display_bpms = [135.0; 2];
                context.song_timing_bpms = vec![(0.0, 135.0), (4.0, 180.0)];
                if timing_clock {
                    context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                        0.0, 0.0, &deadsync_rules::timing::TimingSegments {
                            bpms: context.song_timing_bpms.clone(), ..Default::default()
                        }, &[]));
                }
            });
            let mut scene = compiled.live_scene.take().expect("owned initial song clock");
            let startup = scene.take_startup_output();
            assert!(startup.errors.is_empty(), "{:?}", startup.errors);
            let clocks = scene.lua.globals().get::<Table>("INIT_CLOCKS").unwrap();
            assert_eq!(clocks.raw_len(), 2);
            for clock in clocks.sequence_values::<Table>() {
                let clock = clock.unwrap();
                for index in [1, 2] {
                    assert_eq!(clock.get::<f64>(index).unwrap(), 10.0,
                        "live Init sees native SongPosition Reset BPS before first timing update");
                }
                for index in [3, 4] { assert_eq!(clock.get::<f64>(index).unwrap(), 0.0); }
            }
            for (beat, seconds, bps) in [(2.25, 1.0, 2.25), (4.75, 2.0277778, 3.0)] {
                let output = scene.advance(SongLuaLiveFrame { beat, music_seconds: seconds,
                    screen_seconds: seconds, delta_seconds: 0.01, ..Default::default() }).unwrap();
                assert!(output.errors.is_empty(), "{:?}", output.errors);
                let clock = scene.lua.globals().get::<Table>("CLOCK").unwrap();
                for index in [1, 2] { assert_eq!(clock.get::<f64>(index).unwrap(), bps); }
            }
        }
        let lua = mlua::Lua::new();
        let mut preview = SongLuaCompileContext::new(".", "preview BPS");
        preview.song_display_bpms = [135.0; 2];
        let runtime = crate::create_song_runtime_table(&lua, &preview).unwrap();
        assert_eq!(runtime.get::<f64>(crate::SONG_LUA_RUNTIME_BPS_KEY).unwrap(), 2.25,
            "nonlive preview retains display BPS");
    }

    #[test]
    fn live_screen_begin_reaches_message_commands_only_in_screen_children() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-begin-message-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("begin message directory");
        let background = dir.join("background.lua");
        let foreground = dir.join("foreground.lua");
        fs::write(&background, r#"
BG_BEGIN = 0
return Def.ActorFrame { Def.Actor {
    BeginMessageCommand = function() BG_BEGIN = BG_BEGIN + 1 end,
} }
"#).unwrap();
        fs::write(&foreground, r#"
FG_BEGIN, TARGETS = 0, 0
local proxy = Def.ActorProxy {
    InitCommand = function(self) PROXY = self end,
    BeginMessageCommand = function(self)
        FG_BEGIN = FG_BEGIN + 1
        if TARGET then self:SetTarget(TARGET); TARGETS = TARGETS + 1 end
    end,
}
return Def.ActorFrame { proxy, Def.ActorProxy {
    BeginMessageCommand = function(self) FG_BEGIN = FG_BEGIN + 1 end,
} }
"#).unwrap();
        let mut context = SongLuaCompileContext::new(&dir, "native begin aliases");
        context.live = true;
        context.background_layer_count = 1;
        context.layer_start_beats = vec![0.0, 0.0];
        context.foreground_names = vec!["foreground".into()];
        let mut layers: Vec<Compiled> = crate::compile_song_lua_layers_with_default_host(
            &[&background, &foreground], 1, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("owned background and foreground");
        let mut scene = layers[1].live_scene.take().expect("primary scene");
        let startup = scene.take_startup_output();
        assert!(startup.errors.is_empty(), "{:?}", startup.errors);
        let globals = scene.lua.globals();
        assert_eq!(globals.get::<u32>("FG_BEGIN").unwrap(), 2,
            "screen Begin invokes native foreground BeginMessageCommand aliases");
        assert_eq!(globals.get::<u32>("BG_BEGIN").unwrap(), 0,
            "cached background owners are outside screen Begin propagation");
        assert_eq!(globals.get::<u32>("TARGETS").unwrap(), 0);
        assert!(scene.lua.load("return type(PROXY:GetCommand('Begin')) == 'function'").eval::<bool>().unwrap());
        scene.lua.load("PROXY:playcommand('Begin')").exec().unwrap();
        assert_eq!(globals.get::<u32>("FG_BEGIN").unwrap(), 3);
        assert_eq!(globals.get::<u32>("BG_BEGIN").unwrap(), 0);
        assert!(startup.broadcasts.iter().all(|(_, name, _)| name != "Begin"),
            "direct screen command creates no synthetic Begin broadcast");
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn live_numeric_option_getter_retains_native_argument_path() {
        let mut compiled = compile("numeric-option-path", "return Def.Actor{}");
        let scene = compiled.live_scene.take().expect("owned option state");
        let game = scene.lua.globals().get::<Table>("GAMESTATE").unwrap();
        let state = game.get::<Function>("GetPlayerState").unwrap()
            .call::<Table>((game, "PlayerNumber_P1")).unwrap();
        let original = state.get::<Function>("GetPlayerOptions").unwrap()
            .call::<Table>((state.clone(), "ModsLevel_Current")).unwrap();
        scene.lua.set_app_data(QueryTrace {
            objects: vec![
                (state.clone(), serde_json::json!({"type":"PlayerState", "path":"player-state:PlayerNumber_P1"})),
                (original.clone(), serde_json::json!({"type":"PlayerOptions", "path":"player-state:PlayerNumber_P1/options:ModsLevel_Current"})),
            ],
            ..Default::default()
        });
        hook_query(&scene.lua, &state, "PlayerState", "GetPlayerOptions").unwrap();
        let numeric = state.get::<Function>("GetPlayerOptions").unwrap()
            .call::<Table>((state.clone(), 3)).unwrap();
        assert_eq!(numeric.to_pointer(), original.to_pointer(), "numeric level addresses the existing option object");
        assert_eq!(query_value(&scene.lua, &Value::Table(numeric.clone()), 0).unwrap(),
            serde_json::json!({"type":"PlayerOptions", "path":"player-state:PlayerNumber_P1/options:3"}),
            "numeric ModsLevel query retains the native raw argument path");
        numeric.get::<Function>("Reverse").unwrap().call::<MultiValue>((numeric.clone(), 0.5)).unwrap();
        let symbolic = state.get::<Function>("GetPlayerOptions").unwrap()
            .call::<Table>((state, "ModsLevel_Current")).unwrap();
        assert_eq!(symbolic.to_pointer(), numeric.to_pointer(), "getter aliases preserve object identity");
        assert_eq!(symbolic.get::<Function>("Reverse").unwrap().call::<f32>(symbolic.clone()).unwrap(), 0.5);
        assert_eq!(query_value(&scene.lua, &Value::Table(symbolic), 0).unwrap(),
            serde_json::json!({"type":"PlayerOptions", "path":"player-state:PlayerNumber_P1/options:ModsLevel_Current"}),
            "a later symbolic getter refreshes the same native object path");
    }

    #[test]
    fn live_hsva_initializes_native_vertex_colors() {
        let mut compiled = compile("native-hsva-vertices", r#"
return Def.ActorMultiVertex {
    Name = 'SpaceshipTrailP1',
    InitCommand = function(self)
        assert(type(HSVA) == 'function', 'native HSVA is available for authored vertex initialization')
        local inputs = {
            {0, 1, 1, 1}, {60, 1, 1, 0.75}, {120, 1, 1, 0.5},
            {180, 1, 1, 0.25}, {240, 1, 1, 0}, {300, 1, 1, 1},
            {360, 1, 1, 0.5}, {-30, 1, 1, 0.25}, {480, 1, 1, 1},
            {15, 0.5, 0.25, 0.125}, {270, 0, 0.375, -0.25}, {30, 2, 0.5, 1.25},
        }
        local vertices = {}
        HSVA_COLORS = {}
        for index, input in ipairs(inputs) do
            local rgba = HSVA(unpack(input))
            HSVA_COLORS[index] = rgba
            vertices[index] = {{index, index % 2, 0}, rgba}
        end
        assert(HSVToColor{Hue=0, Sat=0, Value=0.5}[4] == 0)
        assert(HSVToColor{Hue=0, Sat=0, Value=0.5, Alpha=false}[4] == 0)
        assert(HSVA(0, 1, 1)[4] == 0)
        self:SetDrawState{Mode='DrawMode_Quads'}:SetVertices(vertices)
    end,
}
"#);
        let expected: [[f64; 4]; 12] = [
            [1.0, 0.0, 0.0, 1.0], [1.0, 1.0, 0.0, 0.75], [0.0, 1.0, 0.0, 0.5],
            [0.0, 1.0, 1.0, 0.25], [0.0, 0.0, 1.0, 0.0], [1.0, 0.0, 1.0, 1.0],
            [1.0, 0.0, 1.0, 0.5], [1.0, 0.0, 0.5, 0.25], [1.0, 0.0, 1.0, 1.0],
            [0.25, 0.15625, 0.125, 0.125], [0.375, 0.375, 0.375, -0.25], [0.5, 0.0, -0.5, 1.25],
        ];
        let mut scene = compiled.live_scene.take().expect("owned vertex color scene");
        assert!(scene.take_startup_output().errors.is_empty());
        let colors = scene.lua.globals().get::<Table>("HSVA_COLORS").unwrap();
        for (index, expected) in expected.iter().enumerate() {
            let rgba = colors.raw_get::<Table>(index + 1).unwrap();
            for (channel, expected) in expected.iter().enumerate() {
                assert_eq!(rgba.raw_get::<f64>(channel + 1).unwrap(), *expected,
                    "native Lua color {index}, channel {channel}");
            }
        }
        let SongLuaOverlayKind::ActorMultiVertex { vertices, .. } = &compiled.overlays[0].kind else {
            panic!("the authored trail retains its vertex mesh");
        };
        assert_eq!(vertices.len(), 18);
        let mut seen = [false; 12];
        for vertex in vertices.iter() {
            let index = vertex.pos[0] as usize;
            assert!((1..=expected.len()).contains(&index), "authored vertex position retained");
            assert_eq!(vertex.color, expected[index - 1].map(|v| v as f32));
            seen[index - 1] = true;
        }
        assert!(seen.into_iter().all(|seen| seen), "all authored colors reach the rendered mesh");
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn live_message_order_gaps_preserve_callback_dispatch() {
        let mut compiled = compile_with_context("sole-native-judgment-subscriber",
            "return Def.Actor{}", |context| {
                context.defer_initial_live_update = true;
                context.message_actor_order = vec![("1".into(), 10)];
            });
        let mut scene = compiled.live_scene.take().expect("retained theme observer");
        // Keep this registry-order corner case a genuinely lone subscriber.
        // The newly loaded native ColumnFlash actors are separate unit fixtures.
        scene.lua.load(r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
for _,pn in ipairs{'P1','P2'} do
    local container=underlay:GetChild('NoteFieldContainer'..pn)
    if container then rawset(container:GetChild('')[1], 'JudgmentMessageCommand', nil) end
end
"#).exec().expect("isolate original lone Judgment subscriber fixture");
        for actor in crate::song_lua_actor_registry(&scene.lua).unwrap().sequence_values::<Table>() {
            let actor = actor.unwrap();
            if let Value::Function(callback) = actor.raw_get::<Value>("JudgmentMessageCommand").unwrap()
                && !callback.info().source.is_some_and(|source| source.ends_with("FailOnHoldStart.lua"))
            {
                actor.raw_set("JudgmentMessageCommand", Value::Nil).unwrap();
            }
        }
        let mut frame = SongLuaLiveFrame {
            music_seconds: 60.0, screen_seconds: 60.0, beat: 120.0,
            delta_seconds: 0.01, ..Default::default()
        };
        frame.players[0].life = 0.5;
        frame.players[0].autoplay = true;
        frame.players[0].fail_enabled = true;
        let output = scene.advance(frame.clone()).expect("owned gameplay frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let subscribers = crate::song_lua_actor_registry(&scene.lua).unwrap()
            .sequence_values::<Table>().filter(|actor|
                matches!(actor.as_ref().unwrap().raw_get::<Value>("JudgmentMessageCommand").unwrap(), Value::Function(_)))
            .count();
        assert_eq!(subscribers, 1, "only the owned FailOnHoldStart listener subscribes");
        let params = scene.lua.create_table().unwrap();
        params.raw_set("Player", "PlayerNumber_P1").unwrap();
        let result = crate::lua_util::broadcast_native_song_lua_message(
            &scene.lua, "Judgment", Some(Value::Table(params)));
        assert!(result.is_ok(),
            "a lone native Judgment subscriber is dispatched without a sort rank: {result:?}");
        scene.lua.load("SCREENMAN:GetTopScreen():GetChild('Overlay'):playcommand('Off')")
            .exec().expect("owned overlay Off");
        assert_eq!(take_output(&scene.lua).unwrap().player_writes,
            vec![SongLuaLivePlayerWrite::Fail { player: 0 }],
            "the real Judgment latch fails autoplay during overlay Off");

        let mut compiled = compile_with_context("multiple-captured-subscribers", r#"
CALLS = 0
return Def.ActorFrame {
    ProbeMessageCommand = function() CALLS = CALLS + 1 end,
    Def.Actor {ProbeMessageCommand = function() CALLS = CALLS + 1 end},
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.message_actor_order = vec![("1".into(), 10)];
        });
        let scene = compiled.live_scene.take().unwrap();
        let result = crate::lua_util::broadcast_native_song_lua_message(&scene.lua, "Probe", None);
        assert!(result.is_ok(),
            "missing captured ordering must not abort callback dispatch: {result:?}");
        assert_eq!(scene.lua.globals().get::<u32>("CALLS").unwrap(), 2,
            "both subscribed callbacks run when an ordering identity is absent");
        let output = take_output(&scene.lua).unwrap();
        assert!(output.errors.is_empty());
        assert_eq!(output.coverage_gaps,
            vec!["missing captured subscriber identity for 1/1"]);
        crate::lua_util::broadcast_native_song_lua_message(&scene.lua, "Probe", None).unwrap();
        assert_eq!(scene.lua.globals().get::<u32>("CALLS").unwrap(), 4);
        assert!(take_output(&scene.lua).unwrap().coverage_gaps.is_empty(),
            "a recurring ordering gap is reported once per scene");

        let mut compiled = compile_with_context("partially-captured-order", r#"
SEEN = {}
return Def.ActorFrame {
    InitCommand = function(self) ROOT = self end,
    ProbeMessageCommand = function() table.insert(SEEN, 'root') end,
    Def.Actor {ProbeMessageCommand = function() table.insert(SEEN, 'first') end},
    Def.Actor {ProbeMessageCommand = function() table.insert(SEEN, 'second') end},
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.message_actor_order = vec![("1/1".into(), 20), ("1/2".into(), 10)];
        });
        let scene = compiled.live_scene.take().unwrap();
        let root = scene.lua.globals().get::<Table>("ROOT").unwrap().to_pointer() as usize;
        let mut pointers: Vec<_> = crate::song_lua_actor_registry(&scene.lua).unwrap()
            .sequence_values::<Table>().map(Result::unwrap)
            .filter(|actor| matches!(actor.get::<Value>("ProbeMessageCommand").unwrap(), Value::Function(_)))
            .map(|actor| actor.to_pointer() as usize).collect();
        pointers.sort_unstable();
        let root_slot = pointers.iter().position(|pointer| *pointer == root).unwrap();
        crate::lua_util::broadcast_native_song_lua_message(&scene.lua, "Probe", None).unwrap();
        let seen: Vec<_> = scene.lua.globals().get::<Table>("SEEN").unwrap()
            .sequence_values::<String>().map(Result::unwrap).collect();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[root_slot], "root", "the unknown subscriber keeps its VM identity slot");
        assert_eq!(seen.into_iter().filter(|name| name != "root").collect::<Vec<_>>(),
            vec!["second", "first"], "captured listeners retain their native relative order");
        assert_eq!(take_output(&scene.lua).unwrap().coverage_gaps,
            vec!["missing captured subscriber identity for 1"]);

        let mut compiled = compile_with_context("mixed-theme-judgment-subscribers",
            "CALLS = 0; return Def.Actor {JudgmentMessageCommand = function() CALLS = CALLS + 1 end}",
            |context| {
                context.defer_initial_live_update = true;
                context.message_actor_order = vec![("1".into(), 10)];
            });
        let mut scene = compiled.live_scene.take().unwrap();
        scene.advance(frame).unwrap();
        let params = scene.lua.create_table().unwrap();
        params.raw_set("Player", "PlayerNumber_P1").unwrap();
        params.raw_set("TapNoteScore", "TapNoteScore_W1").unwrap();
        crate::lua_util::broadcast_native_song_lua_message(
            &scene.lua, "Judgment", Some(Value::Table(params))).unwrap();
        scene.lua.load("SCREENMAN:GetTopScreen():GetChild('Overlay'):playcommand('Off')")
            .exec().unwrap();
        let output = take_output(&scene.lua).unwrap();
        assert!(output.errors.is_empty());
        assert_eq!(scene.lua.globals().get::<u32>("CALLS").unwrap(), 1);
        assert_eq!(output.player_writes, vec![SongLuaLivePlayerWrite::Fail { player: 0 }],
            "the owned Judgment latch still fails autoplay with an authored subscriber");
        assert!(output.coverage_gaps.iter().any(|gap| gap.contains("ScreenGameplay/")),
            "missing native theme ordering remains explicit coverage");
    }

    #[test]
    fn live_background_sprite_keeps_declared_texture_for_upper_left() {
        let mut compiled = compile_with_context("background-upper-left-texture", r#"
return Def.Sprite {
    Texture=GAMESTATE:GetCurrentSong():GetSongDir()..'/image.png',
    InitCommand=function(self) BG_SPRITE = self end,
    OnCommand=function(self) self:aux(31) end,
}
"#, |context| {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([255,255,255,255]))
                .save(context.song_dir.join("image.png")).unwrap();
            context.background_layer_count = 1;
            context.layer_start_beats = vec![10.0];
            context.background_upper_left_colors = vec![Some([1.0;4])];
        });
        let mut scene = compiled.live_scene.take().unwrap();
        let sprite = scene.lua.globals().get::<Table>("BG_SPRITE").unwrap();
        assert!(sprite.get::<Function>("GetTexture").unwrap().call::<Value>(sprite.clone()).unwrap().is_table());
        assert!(sprite.get::<Function>("GetParent").unwrap().call::<Value>(sprite.clone()).unwrap().is_table(),
            "native UpperLeft owns its returned root inside a real ActorFrame");
        let output=scene.advance(SongLuaLiveFrame { beat: 10.0, music_seconds: 10.0,
            screen_seconds: 10.0, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!(sprite.get::<Function>("GetTexture").unwrap().call::<Value>(sprite).unwrap().is_table());
    }

    #[test]
    fn live_background_start_time_uses_native_note_row() {
        let mut compiled = compile_with_context("background-rounded-start", r#"
return Def.Actor {
    InitCommand = function(self) BACKGROUND = self end,
    OnCommand = function(self) self:sleep(0.004):queuecommand('Done') end,
    DoneCommand = function(self) self:aux(self:getaux()+1) end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.background_layer_count = 1;
            context.layer_start_beats = vec![0.01];
            context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                0.0, 0.0, &deadsync_rules::timing::TimingSegments {
                    bpms: vec![(0.0, 120.0)], ..Default::default()
                }, &[]));
        });
        let mut scene = compiled.live_scene.take().expect("retained background");
        // The authored marker selects the activation; elapsed time rounds
        // that marker to note row zero, consuming all 6ms on the first update.
        let output = scene.advance(SongLuaLiveFrame {
            music_seconds: 0.006, beat: 0.012, screen_seconds: 0.006,
            delta_seconds: 0.001, ..Default::default()
        }).expect("background activation frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let actor = scene.lua.globals().get::<Table>("BACKGROUND").expect("owned background");
        assert_eq!(actor.get::<Function>("getaux").unwrap().call::<f32>(actor).unwrap(), 1.0,
            "background consumes delta from the native rounded start row");
    }

    #[test]
    fn live_native_float_foreground_activation_clock() {
        use deadsync_rules::timing::{TimingData, TimingSegments};
        for start_beat in [17.25f32,20.5,21.25,44.75] {
          for music_rate in [1.0f32,2.0] {
            let bpms=vec![(0.000000f32,71.697998f32),
                (1.000000f32,71.695999f32),
                (2.000000f32,74.014000f32),
                (3.000000f32,74.012001f32),
                (4.000000f32,72.457001f32),
                (5.000000f32,72.455002f32),
                (6.000000f32,72.969002f32),
                (8.000000f32,72.704002f32),
                (10.000000f32,72.622002f32),
                (12.000000f32,71.123001f32),
                (14.000000f32,73.051003f32),
                (18.000000f32,73.054001f32),
                (19.000000f32,73.051003f32),
                (20.000000f32,71.791000f32),
                (21.000000f32,71.789001f32),
                (22.000000f32,72.101997f32),
                (24.000000f32,72.457001f32),
                (26.000000f32,73.051003f32),
                (28.000000f32,68.597000f32),
                (30.000000f32,76.014999f32),
                (31.000000f32,76.013000f32),
                (32.000000f32,70.313004f32),
                (34.000000f32,73.402000f32),
                (36.000000f32,70.874001f32),
                (37.000000f32,70.872002f32),
                (38.000000f32,71.204002f32),
                (39.000000f32,71.202003f32),
                (40.000000f32,69.674004f32),
                (41.000000f32,69.671997f32),
                (42.000000f32,74.014000f32),
                (43.000000f32,74.012001f32),
                (44.000000f32,72.718002f32)];
            let song_offset=-0.532426f32;
            let group_offset=-0.009f32;
            let global_offset=-0.008f32;
            let mut elapsed=-song_offset-group_offset;
            for (index,&(beat,bpm)) in bpms.iter().enumerate() {
                let end=bpms.get(index+1).map_or(start_beat,|point|point.0.min(start_beat));
                if end>beat {elapsed+=(end-beat)/(bpm/60.0);}
            }
            // Captured native build a9d70b63 uses TimingData.cpp e93614b8:
            // GetElapsedTimeFromBeatNoOffset initializes last_time with the
            // separate song/group offsets before traversing timing events.
            let native_start=elapsed-music_rate*global_offset;
            let mut timing=TimingData::from_segments(-song_offset,global_offset,
                &TimingSegments {bpms:bpms.clone(),..Default::default()},&[]);
            timing.shift_group_offset_seconds(group_offset);
            assert_eq!(timing.get_song_time_for_beat(start_beat,music_rate).to_bits(),native_start.to_bits(),
                "captured native offsets initialize float event clock");
            let mut compiled=compile_with_context(&format!("foreground-float-clock-{start_beat}-{music_rate}"),r#"
return Def.ActorFrame {
    Def.Actor {OnCommand=function(self) self:sleep(999) end},
    Def.Actor {
        InitCommand=function(self) FOREGROUND=self end,
        OnCommand=function(self) self:aux(1):sleep(1/128):queuecommand('Done') end,
        DoneCommand=function(self) self:aux(2) end,
    },
}
"#,|context| {
                context.defer_initial_live_update=true;
                context.layer_start_beats=vec![start_beat];
                context.song_timing=Some(timing);
                context.song_timing_bpms=bpms;
                context.song_music_rate=music_rate;
            });
            let mut scene=compiled.live_scene.take().expect("foreground float event-clock scene");
            let first=native_start+music_rate/128.0;
            let next=f32::from_bits(first.to_bits()+1);
            for (music,delta) in [(first,0.01f32),(next,next-first)] {
                let output=scene.advance(SongLuaLiveFrame {music_seconds:f64::from(music),
                    screen_seconds:f64::from(music),delta_seconds:f64::from(delta),
                    beat:f64::from(start_beat+0.01),..Default::default()}).unwrap();
                assert!(output.errors.is_empty(),"{:?}",output.errors);
            }
            let actor=scene.lua.globals().get::<Table>("FOREGROUND").unwrap();
            assert_eq!(actor.get::<Function>("getaux").unwrap().call::<f32>(actor).unwrap(),2.0,
                "foreground command must start from native float timing");
          }
        }
    }

    #[test]
    fn live_native_float_scale_operation_order() {
        let compiled=compile("native-scale-operation-order", "return Def.Actor {}");
        let scene=compiled.live_scene.expect("native scale host");
        let width=480.0f32*(16.0f32/9.0f32);
        let actual=scene.lua.globals().get::<Function>("scale").unwrap()
            .call::<f32>((width,640.0f32,854.0f32,-8.0f32,20.0f32)).unwrap();
        // RageUtil.h SCALE multiplies before dividing. LuaManager.cpp casts
        // all five inputs to float before invoking that macro.
        let native=(width-640.0f32)*(20.0f32-(-8.0f32))/(854.0f32-640.0f32)+(-8.0f32);
        assert_eq!(actual.to_bits(),native.to_bits(),
            "native SCALE must multiply before dividing without fused addition");
    }

    #[test]
    fn live_foreground_start_time_uses_native_note_row() {
        let mut compiled = compile_with_context("foreground-rounded-start", r#"
return Def.Actor {
    InitCommand = function(self) FOREGROUND = self end,
    OnCommand = function(self) self:sleep(0.004):queuecommand('Done') end,
    DoneCommand = function(self) self:aux(self:getaux()+1) end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.01];
            context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                0.0, 0.0, &deadsync_rules::timing::TimingSegments {
                    bpms: vec![(0.0, 120.0)], ..Default::default()
                }, &[]));
        });
        let mut scene = compiled.live_scene.take().expect("retained foreground");
        // Foreground checks the authored 0.01 beat, then GetElapsedTimeFromBeat
        // rounds its marker to row zero. Its first update consumes all 6ms.
        let output = scene.advance(SongLuaLiveFrame {
            music_seconds: 0.006, beat: 0.012, screen_seconds: 0.006,
            delta_seconds: 0.001, ..Default::default()
        }).expect("foreground activation frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let actor = scene.lua.globals().get::<Table>("FOREGROUND").expect("owned foreground");
        assert_eq!(actor.get::<Function>("getaux").unwrap().call::<f32>(actor).unwrap(), 1.0,
            "foreground consumes delta from the native rounded start row");
    }

    #[test]
    fn live_sprite_linear_frames_builds_native_authored_states() {
        let mut compiled = compile_with_context("sprite-linear-frames", r#"
assert(type(Sprite.LinearFrames) == 'function', 'native Sprite.LinearFrames helper is available')
local seconds = 184/60*2
local frames = Sprite.LinearFrames(144, seconds)
assert(#frames == 144)
for i, entry in ipairs(frames) do
    assert(entry.Frame == i-1 and entry.Delay == (1/144)*seconds)
end
assert(#Sprite.LinearFrames(0, 1) == 0)
local other = Sprite.LinearFrames(3, 1.5)
assert(#other == 3 and other[1].Frame == 0 and other[3].Frame == 2)
assert(other[1].Delay == 0.5 and other[3].Delay == 0.5)
other[1].Frame = 99
assert(Sprite.LinearFrames(3, 1.5)[1].Frame == 0)
return Def.Sprite {
    Frames = frames,
    InitCommand = function(self) FRAME_SPRITE = self end,
}
"#, |context| context.defer_initial_live_update = true);
        let scene = compiled.live_scene.take().expect("live sprite declaration");
        let actor = scene.lua.globals().get::<Table>("FRAME_SPRITE").expect("initialized authored sprite");
        let states = crate::lua_util::read_sprite_states(&actor).expect("native Frames table is consumed");
        assert_eq!(states.len(), 144);
        for (index, state) in states.iter().enumerate() {
            assert_eq!(state.frame, index as u32);
            assert_eq!(state.delay, ((1.0_f64 / 144.0) * (184.0 / 60.0 * 2.0)) as f32);
        }
    }

    #[test]
    fn live_actor_name_ignores_lua_attribute_writes() {
        let mut compiled = compile("native-actor-name", r#"
return Def.ActorFrame{Name='Root',
 Def.ActorFrame{InitCommand=function(self) self.Name='Fish_001' end},
 Def.Actor{Name='Declared',InitCommand=function(self) self.Name='Attribute' end},
 OnCommand=function(self)
  local unnamed=self:GetChildren()['']
  assert(unnamed and unnamed:GetName()=='', 'runtime actor name ignores Lua Name attribute')
  assert(unnamed.Name=='Fish_001')
  assert(self:GetChild('Fish_001')==nil)
  local declared=self:GetChild('Declared')
  assert(declared and declared:GetName()=='Declared')
  assert(declared.Name=='Attribute' and self:GetChild('Attribute')==nil)
  unnamed:name('RuntimeFish')
  assert(unnamed:GetName()=='RuntimeFish' and unnamed.Name=='Fish_001')
  assert(self:GetChild('RuntimeFish')==unnamed)
  unnamed.Name='AnotherAttribute'
  assert(unnamed:GetName()=='RuntimeFish')
  self:aux(1)
 end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        let output = scene.advance(SongLuaLiveFrame::default()).expect("owned update");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let root: Table = scene.root.as_table().expect("session roots").raw_get(1).expect("root");
        assert_eq!(root.get::<Function>("getaux").unwrap().call::<f32>(root.clone()).unwrap(), 1.0);
        let fish: Table = root.raw_get(1).expect("same retained fish");
        let observed = query_value(&scene.lua, &Value::Table(fish), 0).expect("query identity");
        assert_eq!(observed["name"], "RuntimeFish");
        assert_eq!(observed["path"], "ScreenGameplay/SongForeground/Root/RuntimeFish");
    }

    #[test]
    fn actor_query_loading_names() {
        let lua = Lua::new();
        let actor: Table = lua.load(r#"
local screen={__songlua_actor_type='TopScreen',Name='ScreenGameplay'}
return setmetatable({__songlua_actor_type='ActorFrame',Name='Declared',
 __songlua_load_name='',__songlua_parent=screen},
 {__index=function() error('query observation must not run authored lookup') end})
"#).eval().expect("actor with authored lookup");
        let mut value = query_value(&lua, &Value::Table(actor.clone()), 0).expect("loading actor");
        assert_eq!(value["name"], "");
        assert_eq!(value["path"], "ScreenGameplay");
        assert!(value["actor"].is_string());
        actor.raw_set("__songlua_load_name", Value::Nil).expect("finish loading");
        value = query_value(&lua, &Value::Table(actor.clone()), 0).expect("loaded actor");
        assert_eq!(value["name"], "Declared");
        assert_eq!(value["path"], "ScreenGameplay/Declared");
        actor.raw_set("__songlua_parent", actor.clone()).expect("invalid ownership");
        assert!(query_value(&lua, &Value::Table(actor), 0).is_err());
    }

    #[test]
    fn stage_failure_is_visible_without_life_failure() {
        let mut player = deadsync_gameplay::init_player_runtime();
        assert!(!super::SongLuaLivePlayer::from_player(&player, 100).failed);
        player.stage_failed = true;
        let frame = super::SongLuaLivePlayer::from_player(&player, 100);
        assert!(frame.failed);
        assert_eq!(frame.life, 0.5);
        assert_eq!(frame.actual_points, 0);
        assert_eq!(player.fail_time, None);
        assert!(!player.is_failing);
    }

    use super::*;
    use std::{fs, path::Path, sync::Arc};
    use crate::{SongLuaDifficulty, SongLuaNoteskinResolver, SongLuaOverlayActor, SongLuaOverlayKind};

    type Compiled = crate::CompiledSongLua<SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>>;

    fn compile(name: &str, source: &str) -> Compiled {
        compile_with_context(name, source, |_| {})
    }

    #[test]
    fn live_scalar_writes_and_idle_frames_preserve_owned_getter_state() {
        let mut compiled = compile_with_context("scalar-publication", r#"
return Def.Actor { InitCommand=function(self)
    ROOT=self; self:x(10):y(20):zoom(1):diffuse(0.2,0.3,0.4,1)
end }
"#, |context| {
            context.players[0].step_statistics_enabled = false;
            context.players[1].enabled = false;
        });
        let mut scene = compiled.live_scene.take().unwrap();
        assert!(!scene.has_custom_draw().unwrap());
        let actor: Table = scene.lua.globals().get("ROOT").unwrap();
        let colors = actor.raw_get::<Table>("__songlua_state_diffuse").unwrap();
        scene.lua.load(r#"
ROOT:skewx(0.25):x(11):zoom(1.25)
assert(ROOT:GetX()==11 and ROOT:GetZoomX()==1.25)
ROOT:linear(1):x(99):y(30):skewx(0)
assert(ROOT:GetX()==11 and ROOT:GetY()==20)
assert(ROOT:GetDestX()==99 and ROOT:GetDestY()==30)
"#).exec().unwrap();
        assert_eq!(actor.raw_get::<Table>("__songlua_state_diffuse").unwrap().to_pointer(), colors.to_pointer(),
            "scalar writes must preserve the unchanged owned color snapshot");
        let frame = SongLuaLiveFrame { music_seconds:0.5,screen_seconds:0.5,beat:0.5,
            delta_seconds:0.5,..Default::default() };
        assert!(scene.advance(frame.clone()).unwrap().errors.is_empty());
        scene.lua.load("assert(ROOT:GetX()==55 and ROOT:GetY()==25); ROOT:y(50); assert(ROOT:GetY()==25 and ROOT:GetDestY()==50)").exec().unwrap();
        assert!(scene.advance(frame.clone()).unwrap().errors.is_empty());
        scene.lua.load("assert(ROOT:GetX()==99 and ROOT:GetY()==50); ROOT:x(101)").exec().unwrap();
        let colors = actor.raw_get::<Table>("__songlua_state_diffuse").unwrap();
        assert!(scene.advance(frame).unwrap().errors.is_empty());
        assert_eq!(actor.raw_get::<Table>("__songlua_state_diffuse").unwrap().to_pointer(), colors.to_pointer(),
            "an idle tween queue must not allocate and republish an unchanged pose");
        scene.lua.load("assert(ROOT:GetX()==101 and ROOT:GetY()==50 and ROOT:GetZoomX()==1.25)").exec().unwrap();
    }

    #[test]
    fn live_foreground_proxy_targets_reach_prepared_visuals() {
        let mut compiled = compile_with_context("late-proxy-target", r#"
return Def.ActorFrame {
    OnCommand=function(self) self:sleep(1000) end,
    Def.ActorFrameTexture {
        InitCommand=function(self)
            self:SetTextureName('ScreenTex'):SetWidth(640):SetHeight(480):Create()
        end,
        Def.ActorProxy {
            Name='FieldProxy',
            OnCommand=function(self)
                self:SetTarget(SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'))
            end,
        },
    },
    Def.Quad { Name='LocalTarget', InitCommand=function(self) self:visible(false) end },
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].step_statistics_enabled = false;
            context.players[1].enabled = false;
        });
        let proxy = compiled.overlays.iter().position(|a| a.name.as_deref() == Some("FieldProxy")).unwrap();
        let local = compiled.overlays.iter().position(|a| a.name.as_deref() == Some("LocalTarget")).unwrap();
        let bindings = compiled.live_actor_indices().iter().copied().enumerate()
            .map(|(local, global)| (global, SongLuaLiveTarget::Root(local))).collect();
        let mut scene = compiled.live_scene.take().unwrap();
        scene.bind_targets(bindings).unwrap();
        let mut visuals = deadsync_gameplay::SongLuaRuntimeVisuals {
            overlays: compiled.overlays,
            screen_overlay_index: compiled.screen_overlay_index,
            overlay_eases: Vec::new(), overlay_ease_ranges: Vec::new(), overlay_events: Vec::new(),
            background_visual_layers: Vec::new(), foreground_visual_layers: Vec::new(),
            player_actors: compiled.player_actors,
            player_events: Default::default(), player_judgment_events: Default::default(),
            player_combo_events: Default::default(), song_foreground: compiled.song_foreground,
            song_foreground_events: Vec::new(), hidden_players: [false; 2],
            screen_layers: compiled.screen_layers, screen_layer_events: Default::default(),
            note_hides: Default::default(), column_offsets: Default::default(),
            column_splines: Default::default(), screen_width: 640.0, screen_height: 480.0,
        };
        scene.write_visuals(&mut visuals).unwrap();
        assert!(matches!(visuals.overlays[proxy].kind, SongLuaOverlayKind::ActorProxy {
            target: crate::SongLuaProxyTarget::Actor { overlay_index: usize::MAX }
        }), "foreground On has not run during preparation");
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.1, beat: 0.2,
            screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene.write_visuals(&mut visuals).unwrap();
        assert!(matches!(visuals.overlays[proxy].kind, SongLuaOverlayKind::ActorProxy {
            target: crate::SongLuaProxyTarget::NoteField { player_index: 0 }
        }), "late foreground On must bind the actual prepared render slot");
        let owned = &scene.actors[scene.targets.iter().find(|(_, target)|
            matches!(target, SongLuaLiveTarget::Root(index) if *index == proxy)).unwrap().0];
        let target_actor = &scene.actors[scene.targets.iter().find(|(_, target)|
            matches!(target, SongLuaLiveTarget::Root(index) if *index == local)).unwrap().0];
        owned.get::<Function>("SetTarget").unwrap().call::<()>((owned.clone(), target_actor.clone())).unwrap();
        scene.write_visuals(&mut visuals).unwrap();
        assert!(matches!(visuals.overlays[proxy].kind, SongLuaOverlayKind::ActorProxy {
            target: crate::SongLuaProxyTarget::Actor { overlay_index }
        } if overlay_index == local), "runtime retargeting resolves actor identity to the layer's visual index");
        let underlay = scene.lua.load("return SCREENMAN:GetTopScreen():GetChild('Underlay'):visible(false)")
            .eval::<Table>().unwrap();
        owned.get::<Function>("SetTarget").unwrap().call::<()>((owned.clone(), underlay)).unwrap();
        scene.write_visuals(&mut visuals).unwrap();
        assert!(matches!(visuals.overlays[proxy].kind, SongLuaOverlayKind::ActorProxy {
            target: crate::SongLuaProxyTarget::Underlay { hidden: true }
        }), "retargeting also carries the source's current visibility");
    }

    fn compile_with_context(name: &str, source: &str, configure: impl FnOnce(&mut SongLuaCompileContext)) -> Compiled {
        let dir = std::env::temp_dir().join(format!("deadsync-live-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("test directory");
        let entry = dir.join("default.lua");
        fs::write(&entry, source).expect("test script");
        let mut context = SongLuaCompileContext::new(&dir, name);
        context.live = true;
        context.music_length_seconds = 60.0;
        configure(&mut context);
        crate::compile_song_lua_with_default_host(&entry, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("live preparation")
    }

    // These queue-focused unit fixtures finish the native NoteField intro so
    // the asserted ancestor maximum comes only from the actor under test.
    // The complete intro and its options lifecycle are tested separately.
    fn finish_note_field_intro_fixture(scene: &SongLuaLiveScene) {
        scene.lua.load(r#"
local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
for _, pn in ipairs{'P1','P2'} do
    local container = underlay:GetChild('NoteFieldContainer'..pn)
    if container then
        local children = container:GetChild('')
        assert(#children >= 2, 'real ColumnFlash and DisplayMods fixture children')
        children[#children]:finishtweening()
    end
end
"#).exec().expect("finish native NoteField intro in isolated queue fixture");
    }

    #[test]
    fn live_foreground_on_observes_updated_underlay() {
        let mut compiled = compile_with_context("foreground-underlay-order", r#"
local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
return Def.ActorFrame {
    InitCommand = function(self) underlay:sleep(0.5) end,
    OnCommand = function(self)
        self:aux(underlay:GetTweenTimeLeft()):sleep(10)
    end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
        });
        let mut scene = compiled.live_scene.take().expect("scene");
        finish_note_field_intro_fixture(&scene);
        scene.advance(SongLuaLiveFrame { music_seconds: -0.5, beat: -1.0,
            screen_seconds: 0.125, delta_seconds: 0.125, ..Default::default() }).expect("dormant foreground");
        scene.advance(SongLuaLiveFrame { music_seconds: 0.0625, beat: 0.125,
            screen_seconds: 0.1875, delta_seconds: 0.0625, ..Default::default() }).expect("foreground enters");
        assert_eq!(scene.states()[0].aux, 0.3125,
            "Foreground::Update sends On after this frame's underlay update");
    }

    #[test]
    fn live_foreground_observes_transition_before_current_frame_update() {
        let mut compiled = compile_with_context("foreground-transition-order", r#"
IN = SCREENMAN:GetTopScreen():GetChild('In')
LEFT = {}
return Def.ActorFrame {
    OnCommand = function(self)
        self:aux(IN:GetTweenTimeLeft()):sleep(10)
        self:SetUpdateFunction(function()
            LEFT[#LEFT + 1] = IN:GetTweenTimeLeft()
        end)
    end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
        });
        let mut scene = compiled.live_scene.take().expect("scene");
        scene.advance(SongLuaLiveFrame { music_seconds: -0.5, beat: -1.0,
            screen_seconds: 0.125, delta_seconds: 0.125, ..Default::default() }).unwrap();
        scene.advance(SongLuaLiveFrame { music_seconds: 0.0625, beat: 0.125,
            screen_seconds: 0.1875, delta_seconds: 0.0625, ..Default::default() }).unwrap();
        assert_eq!(scene.states()[0].aux, 1.875,
            "foreground On precedes the transition's current-frame Update");
        let left = scene.lua.globals().get::<Table>("LEFT").unwrap();
        assert_eq!(left.raw_get::<f32>(1).unwrap(), 1.875,
            "foreground Update also precedes the transition");
        let transition = scene.lua.globals().get::<Table>("IN").unwrap();
        assert_eq!(crate::actor_tween_time_left(&scene.lua, &transition).unwrap(), 1.8125,
            "the same frame advances the transition after the foreground");
    }

    #[test]
    fn live_empty_callback_slots_preserve_single_update_detection() {
        let mut compiled = compile_with_context("callback-slots", r#"
return Def.ActorFrame {
    Def.ActorFrame { Name = 'Update',
        InitCommand = function(self) self:SetUpdateFunction(function() end) end,
        Def.Quad {},
    },
}
"#, |context| {
            for player in &mut context.players { player.hide_lifebar = true; }
        });
        let mut scene = compiled.live_scene.take().expect("scene");
        // The screen's native BPM callback is independent of the subtree under test.
        let screen = scene.lua.globals().get::<Table>("__songlua_top_screen").unwrap();
        let underlay = crate::actor_children(&scene.lua, &screen).unwrap().raw_get::<Table>("Underlay").unwrap();
        for owner in crate::lua_util::actor_direct_children(&scene.lua, &underlay).unwrap() {
            if crate::actor_children(&scene.lua, &owner).unwrap().raw_get::<Option<Table>>("RatemodDisplay").unwrap().is_some() {
                let inner = crate::lua_util::actor_direct_children(&scene.lua, &owner).unwrap()[1].clone();
                inner.get::<Function>("SetUpdateFunction").unwrap().call::<()>((inner, Value::Nil)).unwrap();
            }
        }
        scene.advance(SongLuaLiveFrame::default()).unwrap();
        assert!(crate::lua_util::actor_tree_update_only(&scene.lua, &scene.root, "Update")
            .expect("only the current update callback counts"));
        let root: Table = scene.root.as_table().expect("session roots").raw_get(1).expect("authored root");
        root.get::<Function>("SetUpdateFunction").expect("update setter")
            .call::<()>((root, scene.lua.create_function(|_, _: MultiValue| Ok(())).expect("extra update")))
            .expect("install second callback");
        assert!(!crate::lua_util::actor_tree_update_only(&scene.lua, &scene.root, "Update")
            .expect("additional callbacks still prevent substitution"));
    }

    #[test]
    fn live_density_graph_owns_quantized_recurring_sleep() {
        for (name, bpms, rate, expected_period) in [
            ("constant", vec![(0.0, 140.0)], 1.0, (30.0_f64 / 140.0 * 2.0) as f32),
            ("changes", vec![(0.0, 140.0), (8.0, 160.0)], 1.0, 0.25),
            ("slow", vec![(0.0, 30.0)], 1.0, 0.25),
            ("rate", vec![(0.0, 140.0)], 2.0, (30.0_f64 / 140.0 * 2.0) as f32),
        ] {
            let mut compiled = compile_with_context(&format!("density-{name}"),
                "return Def.ActorFrame { OnCommand = function(self) self:sleep(10) end }", |context| {
                    context.defer_initial_live_update = true;
                    for player in &mut context.players { player.hide_lifebar = true; }
                    context.screen_width = 854.0;
                    context.players[0].screen_x = 213.5;
                    context.players[1].enabled = false;
                    context.song_timing_bpms = bpms;
                    context.song_music_rate = rate;
                    context.player_timing[0] = Some(deadsync_rules::timing::TimingData::from_segments(
                        0.0, 0.0, &deadsync_rules::timing::TimingSegments {
                            bpms: vec![(0.0, 300.0)], ..Default::default()
                        }, &[]));
                });
            let mut scene = compiled.live_scene.take().expect("scene");
            finish_note_field_intro_fixture(&scene);
            let underlay: Table = crate::actor_children(&scene.lua,
                &scene.lua.globals().get::<Table>("__songlua_top_screen").expect("screen"))
                .expect("screen children").raw_get("Underlay").expect("underlay");
            let pane: Table = crate::actor_children(&scene.lua, &underlay).expect("underlay children")
                .raw_get("StepStatsPaneP1").expect("enabled pane");
            let mut left = 0.0_f32;
            let mut screen = 0.0;
            for delta in [0.0_f32, 0.01, 0.2, 0.3, 0.02] {
                let mut remaining = delta;
                while remaining > 0.0 {
                    if left == 0.0 { left = expected_period; }
                    if remaining < left { left -= remaining; break; }
                    remaining -= left;
                    left = 0.0;
                }
                screen += f64::from(delta);
                scene.advance(SongLuaLiveFrame { screen_seconds: screen,
                    music_seconds: -1.0 + screen, beat: -2.0 + screen * 2.0,
                    delta_seconds: f64::from(delta), ..Default::default() }).expect("density frame");
                assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &pane).expect("pane queue"),
                    left, "{name}: native float queue after delta {delta}");
                assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &underlay).expect("underlay queue"),
                    left, "ancestor reports its owned descendant");
            }
        }
        for (name, width, style) in [("narrow-centered", 640.0, "single"), ("versus", 854.0, "versus")] {
            let mut compiled = compile_with_context(&format!("density-excluded-{name}"), "return Def.ActorFrame {}", |context| {
                context.defer_initial_live_update = true;
                    for player in &mut context.players { player.hide_lifebar = true; }
                context.screen_width = width;
                context.style_name = style.into();
            });
            let mut scene = compiled.live_scene.take().expect("excluded layout");
            finish_note_field_intro_fixture(&scene);
            scene.advance(SongLuaLiveFrame { delta_seconds: 0.01, ..Default::default() }).expect("layout frame");
            let screen: Table = scene.lua.globals().get("__songlua_top_screen").expect("screen");
            let underlay: Table = crate::actor_children(&scene.lua, &screen).expect("screen children")
                .raw_get("Underlay").expect("underlay");
            assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &underlay).expect("excluded graph"), 0.0,
                "{name}: excluded StepStatistics layouts create no recurring queue");
        }
    }

    #[test]
    fn live_overlay_fails_judged_autoplay_during_off_after_exit_decision() {
        for (name, autoplay, judgment) in [("used", true, true), ("unused", true, false), ("human", false, true)] {
            let mut compiled = compile_with_context(&format!("overlay-failure-{name}"), r#"
OFFS, FAILED_AT_OFF = 0, false
return Def.ActorFrame { OffCommand = function(self)
    OFFS = OFFS + 1
    FAILED_AT_OFF = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1):GetFailed()
end }
"#, |context| {
                context.defer_initial_live_update = true;
                context.players[1].enabled = false;
            });
            let mut scene = compiled.live_scene.take().expect("owned scene");
            let mut frame = SongLuaLiveFrame { delta_seconds: 0.1, ..Default::default() };
            frame.players[0].autoplay = autoplay;
            scene.advance(frame.clone()).expect("controller frame");
            assert_eq!(scene.lua.load("return IsAutoplay(PLAYER_1)").eval::<bool>().unwrap(), autoplay);
            if judgment {
                let params = scene.lua.create_table().unwrap();
                params.raw_set("Player", crate::player_number_name(0)).unwrap();
                params.raw_set("TapNoteScore", "TapNoteScore_W1").unwrap();
                crate::lua_util::broadcast_native_song_lua_message(&scene.lua, "Judgment", Some(Value::Table(params))).unwrap();
            }
            // The overlay remembers actual Judgment use, even after autoplay
            // is disabled. Merely selecting autoplay does not fail a player.
            frame.players[0].autoplay = false;
            frame.gameplay_outro = true;
            let boundary = scene.advance(frame.clone()).expect("NotesEnded posted");
            assert!(boundary.player_writes.is_empty());
            assert_eq!(scene.lua.globals().get::<i64>("OFFS").unwrap(), 0);
            let off = scene.advance(frame.clone()).expect("LeaveGameplay after actor update");
            assert_eq!(scene.lua.globals().get::<i64>("OFFS").unwrap(), 1,
                "autoplay fails inside successful Off, without posting BeginFailed");
            assert_eq!(scene.lua.globals().get::<bool>("FAILED_AT_OFF").unwrap(), autoplay && judgment,
                "overlay Off precedes foreground Off");
            assert_eq!(off.player_writes, if autoplay && judgment {
                vec![SongLuaLivePlayerWrite::Fail { player: 0 }]
            } else { vec![] });
            frame.players[0].failed = autoplay && judgment;
            assert!(scene.advance(frame.clone()).unwrap().player_writes.is_empty(), "Off runs once");
            assert_eq!(scene.lua.globals().get::<i64>("OFFS").unwrap(), 1);
        }
    }

    #[test]
    fn live_outro_dispatches_off_after_actor_update_once() {
        for failed in [false, true] {
            let mut compiled = compile_with_context(&format!("outro-off-{failed}"), r#"
OFFS, SEEN = 0, {}
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(10):SetUpdateFunction(function(_, delta)
            if delta > 0 then SEEN[#SEEN + 1] = OFFS end
        end)
    end,
    OffCommand = function(self) OFFS = OFFS + 1; self:stoptweening() end,
    EndCommand = function(self) self:aux(23) end,
}
"#, |context| context.players[1].enabled = false);
            let mut scene = compiled.live_scene.take().expect("scene");
            let mut frame = SongLuaLiveFrame { delta_seconds: 0.125, ..Default::default() };
            frame.players[0].failed = failed;
            for index in 0..5 {
                frame.screen_seconds = (index + 1) as f64 * 0.125;
                frame.gameplay_outro = index >= 1;
                scene.advance(frame.clone()).expect("outro frame");
                let expected = i64::from(index >= if failed { 3 } else { 2 });
                assert_eq!(scene.lua.globals().get::<i64>("OFFS").expect("Off count"), expected,
                    "NotesEnded dispatches after actors; BeginFailed waits another update");
            }
            let seen: Table = scene.lua.globals().get("SEEN").expect("callback observations");
            assert_eq!(seen.raw_get::<i64>(if failed { 4 } else { 3 }).expect("same-frame observation"), 0);
            assert_eq!(seen.raw_get::<i64>(5).expect("following-frame observation"), 1);
            scene.end().expect("End is separate");
            assert_eq!(scene.lua.globals().get::<i64>("OFFS").expect("no repeated Off"), 1);
        }
    }

    #[test]
    fn live_player_life_setter_updates_shared_meter_and_notifies_synchronously() {
        let mut compiled = compile("player-life", r#"
local top = SCREENMAN:GetTopScreen()
local player = top:GetChild('PlayerP1')
local meter = top:GetLifeMeter(PLAYER_1)
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local state = GAMESTATE:GetPlayerState(PLAYER_1)
local done, changed = false, 0
HEALTH_CHANGES = 0
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(60)
        self:SetUpdateFunction(function(_, delta)
            if delta <= 0 or done then return end
            done = true
            assert(math.abs(meter:GetLife() - 0.7) < 0.000001)
            assert(player:SetLife(0.2) == player)
            assert(math.abs(meter:GetLife() - 0.2) < 0.000001)
            assert(math.abs(stats:GetCurrentLife() - 0.2) < 0.000001)
            assert(changed == 1)
            assert(player:SetLife(2) == player and meter:GetLife() == 1 and meter:IsHot())
            assert(player:SetLife(-1) == player and meter:GetLife() == 0 and meter:IsFailing())
            assert(changed == 3)
            assert(state:GetHealthState() == 'HealthState_Alive')
            stats:FailPlayer()
            assert(stats:GetFailed())
        end)
    end,
    LifeChangedMessageCommand = function(_, params)
        assert(params.Player == PLAYER_1 and params.LifeMeter == meter)
        assert(params.LifeMeter:GetLife() >= 0 and params.LifeMeter:GetLife() <= 1)
        assert(state:GetHealthState() == 'HealthState_Alive')
        changed = changed + 1
        local ok, err = pcall(function() player:SetLife(0.8) end)
        assert(not ok and tostring(err):find('LifeChangedMessageCommand'))
    end,
    HealthStateChangedMessageCommand = function(_, params)
        assert(params.PlayerNumber == PLAYER_1)
        assert(params.OldHealthState == 'HealthState_Alive' and params.HealthState == 'HealthState_Dead')
        assert(state:GetHealthState() == 'HealthState_Dead')
        HEALTH_CHANGES = HEALTH_CHANGES + 1
    end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("owned life scene");
        let mut frame = SongLuaLiveFrame { screen_seconds: 0.1, music_seconds: 0.1,
            delta_seconds: 0.1, ..Default::default() };
        frame.players[0].life = 0.7;
        frame.players[0].fail_enabled = true;
        let output = scene.advance(frame.clone()).expect("life frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(scene.lua.globals().get::<u32>("HEALTH_CHANGES").expect("health change"), 1);
        assert_eq!(output.player_writes, vec![
            SongLuaLivePlayerWrite::SetLife { player: 0, life: 0.2 },
            SongLuaLivePlayerWrite::SetLife { player: 0, life: 1.0 },
            SongLuaLivePlayerWrite::SetLife { player: 0, life: 0.0 },
            SongLuaLivePlayerWrite::Fail { player: 0 },
        ]);
    }

    #[test]
    fn live_screen_debug_owns_its_native_tween_clock() {
        let mut compiled = compile("screen-debug", r#"
local top = SCREENMAN:GetTopScreen()
local text = top:GetChild('Debug')
assert(text == top:GetChildren().Debug)
assert(text:GetParent() == top)
assert(tostring(text):find('^BitmapText'))
assert(text:GetText() == '')
assert(text:GetX() == SCREEN_CENTER_X)
assert(text:GetY() == SCREEN_BOTTOM - 116)
assert(text:GetZoom() == 0.75)
LEFT = {}
return Def.ActorFrame { OnCommand = function(self)
    text:linear(1):y(SCREEN_CENTER_Y-60)
    self:SetUpdateFunction(function(_, delta)
        if delta > 0 then LEFT[#LEFT+1] = text:GetTweenTimeLeft() end
    end)
end }
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        for seconds in [0.25, 0.5, 0.75, 1.0] {
            let output = scene.advance(SongLuaLiveFrame { screen_seconds: seconds,
                delta_seconds: 0.25, ..Default::default() }).expect("screen frame");
            assert!(output.errors.is_empty(), "{:?}", output.errors);
        }
        let left = scene.lua.globals().get::<Table>("LEFT").expect("authored tween queries");
        for (index, expected) in [(1, 0.75), (2, 0.5), (3, 0.25), (4, 0.0)] {
            assert_eq!(left.raw_get::<f32>(index).expect("tween left"), expected);
        }
    }

    #[test]
    fn live_deferred_initial_update_uses_first_input_clock_once() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-first-clock-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
UPDATES = {}
return Def.ActorFrame { InitCommand = function(self)
    self:SetUpdateFunction(function(_, delta)
        UPDATES[#UPDATES+1] = { delta, GAMESTATE:GetCurMusicSeconds() }
    end)
end }
"#).unwrap();
        let mut context = SongLuaCompileContext::new(&dir, "first input clock");
        context.live = true;
        context.defer_initial_live_update = true;
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(
            &entry, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).unwrap();
        let mut scene = compiled.live_scene.take().unwrap();
        let updates = scene.lua.globals().get::<Table>("UPDATES").unwrap();
        assert_eq!(updates.raw_len(), 0, "preparation must leave Update(0) to the input clock");
        for (music, delta) in [(-4.5, 0.0), (-4.49, 0.01)] {
            let output = scene.advance(SongLuaLiveFrame { music_seconds: music,
                screen_seconds: delta, delta_seconds: delta, ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
        }
        assert_eq!(updates.raw_len(), 2);
        let first = updates.raw_get::<Table>(1).unwrap();
        assert_eq!(first.raw_get::<f64>(1).unwrap(), 0.0);
        assert_eq!(first.raw_get::<f64>(2).unwrap(), -4.5);
        assert_eq!(updates.raw_get::<Table>(2).unwrap().raw_get::<f32>(2).unwrap(), -4.49_f32);
    }

    #[test]
    fn live_draw_follows_owned_update_and_replaces_automatic_children() {
        let mut compiled = compile_with_context("live-draw", r#"
DRAWS, NESTED, AUTOMATIC, QUEUED = 0, 0, 0, 0
return Def.ActorFrame {
    Name='DrawRoot',
    OnCommand=function(self)
        ROOT = self
        local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
        local state = GAMESTATE:GetPlayerState('PlayerNumber_P1')
        self:SetUpdateFunction(function(actor) actor:aux(actor:getaux()+1) end)
        self:SetDrawFunction(function(actor)
            assert(actor:getaux() > DRAWS, 'draw must follow the owned actor update')
            assert(state:GetHealthState() == 'HealthState_Dead', 'draw must see the updated health')
            DRAWS = DRAWS + 1
            actor:GetChild('Nested'):Draw()
            actor:queuecommand('AfterDraw')
        end)
        ORIGINAL_DRAW = self:GetDrawFunction()
        player:SetLife(0)
    end,
    AfterDrawCommand=function() QUEUED = QUEUED + 1 end,
    Def.ActorFrame {
        Name='Nested',
        InitCommand=function(self)
            self:SetDrawFunction(function(actor)
                NESTED = NESTED + 1
                actor:GetChild('Leaf'):x(NESTED):Draw()
            end)
        end,
        Def.Quad { Name='Leaf' },
    },
    Def.ActorFrame {
        Name='Automatic',
        InitCommand=function(self)
            self:SetDrawFunction(function() AUTOMATIC = AUTOMATIC + 1 end)
        end,
    },
}
"#, |context| context.defer_initial_live_update = true);
        let mut scene = compiled.live_scene.take().expect("owned draw scene");
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 0.1,
            music_seconds: 0.1, beat: 0.1, delta_seconds: 0.1,
            players: [SongLuaLivePlayer { fail_enabled: true, ..Default::default() }; LUA_PLAYERS],
            ..Default::default() })
            .expect("first update and draw");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 1,
            "the live frame must execute its installed draw callback");
        assert_eq!(scene.lua.globals().get::<u32>("NESTED").unwrap(), 1,
            "explicit Draw must execute the nested callback");
        assert_eq!(scene.lua.globals().get::<u32>("AUTOMATIC").unwrap(), 0,
            "custom draw replaces automatic child traversal");
        assert_eq!(scene.lua.globals().get::<u32>("QUEUED").unwrap(), 0,
            "draw must leave queued commands for the next actor update");

        let frame = SongLuaLiveFrame { screen_seconds: 0.2, music_seconds: 0.2,
            beat: 0.2, delta_seconds: 0.1,
            players: [SongLuaLivePlayer { fail_enabled: true, ..Default::default() }; LUA_PLAYERS],
            ..Default::default() };
        scene.update(frame.clone()).expect("update without draw");
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 1);
        assert_eq!(scene.lua.globals().get::<u32>("QUEUED").unwrap(), 1);
        scene.draw().expect("separate draw");
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 2);

        scene.lua.load("ROOT:visible(false)").exec().unwrap();
        scene.advance(frame.clone()).unwrap();
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 2);
        scene.lua.load("ROOT:visible(true):hibernate(0.25)").exec().unwrap();
        scene.advance(frame.clone()).unwrap();
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 2);
        scene.lua.load("ROOT:hibernate(0):diffusealpha(0)").exec().unwrap();
        scene.advance(frame.clone()).unwrap();
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 2,
            "transparent actors do not reach native DrawPrimitives");
        scene.lua.load("ROOT:glow(1,1,1,1)").exec().unwrap();
        scene.advance(frame.clone()).unwrap();
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 3,
            "glow makes a transparent actor partially opaque");

        scene.lua.load("ROOT:diffusealpha(1):glow(0,0,0,0):AddWrapperState(); ROOT:GetWrapperState(1):visible(false)").exec().unwrap();
        scene.advance(frame.clone()).unwrap();
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 3,
            "an invisible wrapper aborts the nested draw");
        scene.lua.load("ROOT:GetWrapperState(1):visible(true); ROOT:SetDrawFunction(nil)").exec().unwrap();
        scene.advance(frame.clone()).unwrap();
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), 3);
        assert_eq!(scene.lua.globals().get::<u32>("AUTOMATIC").unwrap(), 1,
            "removing a callback restores ordinary child traversal");
        scene.lua.load("ROOT:SetDrawFunction(function(actor) actor:x(42) end)").exec().unwrap();
        scene.advance(frame.clone()).unwrap();
        assert_eq!(scene.states().iter().find(|state| state.x == 42.0).unwrap().x, 42.0,
            "draw mutations are reflected in the current live visual poses");
        scene.lua.load("ROOT:SetDrawFunction(ORIGINAL_DRAW)").exec().unwrap();
        for index in 0..2000 {
            let output = scene.advance(SongLuaLiveFrame { screen_seconds: 1.0 + index as f64 * 0.1,
                music_seconds: 1.0 + index as f64 * 0.1, beat: 1.0 + index as f64 * 0.1,
                ..frame.clone() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let (frames, calls, bytes) = crate::draw_capture::live_storage(&scene.lua);
            assert_eq!(frames, 1, "draw storage must retain only the current frame");
            assert_eq!(calls, 1, "one explicitly drawn leaf per live frame");
            assert!(bytes < 8192, "live draw storage must not accumulate song history");
        }
        let draws = scene.lua.globals().get::<u32>("DRAWS").unwrap();
        scene.end().unwrap();
        scene.draw().unwrap();
        assert_eq!(scene.lua.globals().get::<u32>("DRAWS").unwrap(), draws,
            "a popped screen does not run another draw callback");
    }

    #[test]
    fn live_loading_screen_is_unavailable_until_begin() {
        for loading in [false, true] {
            let mut compiled = compile_with_context(if loading { "loading-screen" } else { "active-screen" }, r#"
CONSTRUCTION_TOP = SCREENMAN:GetTopScreen()
return Def.ActorFrame {
    InitCommand = function(self)
        INIT_TOP = SCREENMAN:GetTopScreen()
        INIT_PARENT_SCREEN = self:GetParent():GetParent()
    end,
    BeginCommand = function(self) BEGIN_TOP = SCREENMAN:GetTopScreen() end,
    OnCommand = function(self)
        ON_TOP = SCREENMAN:GetTopScreen()
        self:aux(17)
    end,
    EndCommand = function(self) END_TOP = SCREENMAN:GetTopScreen() end,
}
"#, |context| {
                context.screen_loading = loading;
                context.defer_initial_live_update = true;
                context.layer_start_beats = vec![0.0];
            });
            let mut scene = compiled.live_scene.take().expect("screen lifecycle scene");
            let globals = scene.lua.globals();
            let screen = globals.raw_get::<Table>("__songlua_top_screen").unwrap();
            for name in ["CONSTRUCTION_TOP", "INIT_TOP"] {
                let top = globals.raw_get::<Option<Table>>(name).unwrap();
                if loading {
                    assert!(top.is_none(), "{name} must not expose a screen before it is pushed");
                } else {
                    assert_eq!(top.unwrap().to_pointer(), screen.to_pointer());
                }
            }
            assert_eq!(globals.raw_get::<Table>("INIT_PARENT_SCREEN").unwrap().to_pointer(), screen.to_pointer(),
                "the loading screen owns its actors before it becomes the top screen");
            assert_eq!(globals.raw_get::<Table>("BEGIN_TOP").unwrap().to_pointer(), screen.to_pointer());
            let output = scene.advance(SongLuaLiveFrame {
                music_seconds: 0.1, beat: 0.1, screen_seconds: 0.1, delta_seconds: 0.1,
                ..Default::default()
            }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            assert_eq!(globals.raw_get::<Table>("ON_TOP").unwrap().to_pointer(), screen.to_pointer());
            assert_eq!(scene.states()[0].aux, 17.0);
            scene.end().unwrap();
            assert!(globals.raw_get::<Option<Table>>("END_TOP").unwrap().is_none(),
                "ScreenManager pops the screen before End");
        }
    }

    #[test]
    fn live_table_find_returns_native_keys_and_completes_show_arrow() {
        let mut compiled = compile_with_context("table-find-show-arrow", r#"
SHOWS, AFTER_SHOW, SELECTED = 0, false, {}
local fired = false
return Def.ActorFrame {
    OnCommand=function(self)
        assert(table.find({4,8,12}, 8) == 2)
        assert(table.find({[6]='six', tail='end'}, 'six') == 6)
        assert(table.find({[6]='six', tail='end'}, 'end') == 'tail')
        assert(table.find({off=false}, false) == 'off')
        assert(table.find({[false]='needle'}, 'needle') == false)
        assert(table.find({4,8}, '8') == nil)
        assert(table.find({4,8}, nil) == nil and table.find({}, 8) == nil)
        local marker = self:GetChild('Arrow2')
        assert(table.find({self:GetChild('Arrow1'), marker}, marker) == 2)
        local same = {__eq=function(a,b) return a.id == b.id end}
        local a, b = setmetatable({id=7}, same), setmetatable({id=7}, same)
        assert(table.find({[3]=a}, b) == 3)
        self:SetUpdateFunction(function(actor, delta)
            if fired or delta <= 0 then return end
            MESSAGEMAN:Broadcast('ShowArrow', {{2,4},0.5})
            AFTER_SHOW = true
            fired = true
        end)
    end,
    ShowArrowMessageCommand=function(self, args)
        for column=1,4 do
            if table.find(args[1], column) then
                self:GetChild('Arrow'..column):aux(10+column)
                SELECTED[#SELECTED+1] = column
            end
        end
        SHOWS = SHOWS + 1
    end,
    Def.Actor {Name='Arrow1'}, Def.Actor {Name='Arrow2'},
    Def.Actor {Name='Arrow3'}, Def.Actor {Name='Arrow4'},
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
        });
        let mut scene = compiled.live_scene.take().expect("owned ShowArrow scene");
        for seconds in [0.1, 0.2, 0.3] {
            let output = scene.advance(SongLuaLiveFrame { screen_seconds: seconds,
                music_seconds: seconds, beat: seconds, delta_seconds: 0.1,
                ..Default::default() }).expect("ShowArrow frame");
            assert!(output.errors.is_empty(), "{:?}", output.errors);
        }
        assert!(scene.lua.globals().get::<bool>("AFTER_SHOW").unwrap(),
            "the update continues after the real message callback");
        assert_eq!(scene.lua.globals().get::<u32>("SHOWS").unwrap(), 1,
            "successful lookup prevents repeated broadcasts");
        let selected = scene.lua.globals().get::<Table>("SELECTED").unwrap();
        assert_eq!(selected.raw_len(), 2);
        assert_eq!(selected.raw_get::<u32>(1).unwrap(), 2);
        assert_eq!(selected.raw_get::<u32>(2).unwrap(), 4);
    }

    #[test]
    fn live_rec_print_table_handles_owned_actor_values_and_depth() {
        let mut compiled = compile_with_context("recursive-debug-table", r#"
return Def.ActorFrame {
    OnCommand = function(self)
        rec_print_table(nil, 'outer>')
        local cycle = {}; cycle.child = cycle
        rec_print_table(cycle, nil, 0)
        rec_print_table({child={leaf=4}}, '>', 1)
        rec_print_table({child={leaf=4}}, '>', 2)
        rec_print_table({enabled=false})
        local fish = self:GetChild('Fish')
        rec_print_table(fish, 'fish>')
        rec_print_table({item=fish}, 'nested>', 1)
        self:queuecommand('AfterDebug')
    end,
    AfterDebugCommand = function(self) self:aux(19) end,
    Def.Actor { Name='Fish', InitCommand=function(self)
        self.loop = self
        self.parent = self:GetParent()
    end },
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
        });
        let mut scene = compiled.live_scene.take().expect("owned debug scene");
        let messages = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let traced = messages.clone();
        scene.lua.globals().set("Trace", scene.lua.create_function(move |_, message: String| {
            traced.lock().unwrap().push(message);
            Ok(())
        }).expect("trace observer")).expect("observe fallback debug output");
        for seconds in [0.1, 0.2] {
            let output = scene.advance(SongLuaLiveFrame {
                music_seconds: seconds, beat: seconds, screen_seconds: seconds, delta_seconds: 0.1,
                ..Default::default()
            }).expect("debug callback frame");
            assert!(output.errors.is_empty(), "{:?}", output.errors);
        }
        assert!(scene.states().iter().any(|state| state.aux == 19.0),
            "the command after the debug helper runs");
        let messages = messages.lock().unwrap();
        assert_eq!(&messages[..10], [
            "outer>rec_print_table passed a nil", ">child: table", ">end",
            ">child: table", ">  (string)leaf: (number)4", ">  end", ">end",
            "(string)enabled: (boolean)false", "end",
            "fish>rec_print_table passed a userdata",
        ]);
        assert!(messages[10].starts_with("nested>(string)item: (userdata)Actor"),
            "{:?}", messages);
        assert_eq!(messages[11], "nested>end");
        assert_eq!(messages.len(), 12);
    }

    #[test]
    fn live_foreground_directory_uses_requested_default_chunk_name() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-directory-source-{}", std::process::id()));
        let folder = dir.join("directory");
        fs::create_dir_all(&folder).expect("foreground directory");
        let entry = folder.join("Default.lua");
        fs::write(&entry, r#"
CONSTRUCTION_SOURCE = debug.getinfo(1, 'S').source
local function on(self)
    self:aux(17)
end
CALLBACK_SOURCE = debug.getinfo(on, 'S').source
return Def.Actor { OnCommand = on }
"#).expect("case-sensitive physical source");
        for (name, expected) in [
            ("directory", crate::file_path_string(&folder.join("default.lua"))),
            ("directory/", format!("{}//default.lua", crate::file_path_string(&folder))),
            ("./", format!("{}/.//default.lua", crate::file_path_string(&dir))),
            ("directory/Default.lua", crate::file_path_string(&entry)),
            ("./directory/Default.lua", format!("{}/./directory/Default.lua", crate::file_path_string(&dir))),
            ("directory//Default.lua", format!("{}/directory//Default.lua", crate::file_path_string(&dir))),
        ] {
            let physical_entry = if name == "./" {
                let root_entry = dir.join("Default.lua");
                fs::copy(&entry, &root_entry).expect("root directory entry");
                root_entry
            } else { entry.clone() };
            let mut context = SongLuaCompileContext::new(&dir, "foreground source");
            context.live = true;
            context.foreground_names = vec![name.into()];
            context.layer_start_beats = vec![0.0];
            let mut compiled: Compiled = crate::compile_song_lua_with_default_host(
                &physical_entry, &context, SongLuaNoteskinResolver::default(),
                |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None)
                .expect("read physical file with native source name");
            let mut scene = compiled.live_scene.take().expect("retained Lua scene");
            let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.02, beat: 0.01,
                screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() })
                .expect("foreground callback");
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            for key in ["CONSTRUCTION_SOURCE", "CALLBACK_SOURCE"] {
                let source = scene.lua.globals().get::<String>(key).expect("Lua source name");
                assert_eq!(source.trim_start_matches('@').replace('\\', "/"),
                    expected);
            }
            assert!(scene.states().iter().any(|state| state.aux == 17.0));
        }
    }

    #[test]
    fn live_loadactor_uses_helper_source_during_actor_callbacks() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-helper-paths-{}", std::process::id()));
        fs::create_dir_all(dir.join("parts")).unwrap();
        fs::write(dir.join("value.lua"), "error('actor directory must not shadow the calling helper')").unwrap();
        fs::write(dir.join("parts/value.lua"), "local a, b, c = ...; assert(b == nil); return a + c").unwrap();
        fs::write(dir.join("parts/helper.lua"), r#"
return function()
    return LoadActor('value', 23, nil, 5)
end
"#).unwrap();
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
local helper = LoadActor('parts/helper')
return Def.ActorFrame {
    OnCommand = function(self)
        self:aux(helper())
        self:SetUpdateFunction(function(self) self:aux(helper() + 1) end)
    end,
}
"#).unwrap();
        let mut context = SongLuaCompileContext::new(&dir, "callback relative actor");
        context.live = true;
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None, |_, _| None).expect("load helper closure");
        let mut scene = compiled.live_scene.take().expect("retained Lua scene");
        let startup = scene.take_startup_output();
        assert!(startup.errors.is_empty(), "{:?}", startup.errors);
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.01, beat: 0.01,
            screen_seconds: 0.01, delta_seconds: 0.01, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!(scene.states().iter().any(|state| state.aux == 29.0),
            "On and update callbacks load relative to the invoked helper, preserving varargs");
    }

    #[test]
    fn live_loadactor_collapses_paths_before_loading_and_naming_chunks() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-actor-paths-{}", std::process::id()));
        fs::create_dir_all(dir.join("parts")).expect("actor directory");
        fs::write(dir.join("parts/child.lua"), r#"
CHILD_SOURCE = debug.getinfo(1, 'S').source
return Def.ActorFrame { LoadActor('.././sibling.lua') }
"#).expect("nested actor script");
        fs::write(dir.join("sibling.lua"), r#"
SIBLING_SOURCE = debug.getinfo(1, 'S').source
return Def.Actor { OnCommand = function(self) self:aux(19) end }
"#).expect("sibling script");
        let entry = dir.join("default.lua");
        fs::write(&entry, "return Def.ActorFrame { LoadActor('./parts/missing/../child.lua') }")
            .expect("actor path aliases");
        let mut context = SongLuaCompileContext::new(&dir, "actor path collapse");
        context.live = true;
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None, |_, _| None).expect("native collapse happens before checking missing directory");
        let scene = compiled.live_scene.take().expect("retained Lua scene");
        for (key, path) in [("CHILD_SOURCE", dir.join("parts/child.lua")),
            ("SIBLING_SOURCE", dir.join("sibling.lua"))] {
            let source = scene.lua.globals().get::<String>(key).expect("actual Lua chunk source");
            assert_eq!(source.trim_start_matches('@').replace('\\', "/"), crate::file_path_string(&path));
        }
        assert!(scene.states().iter().any(|state| state.aux == 19.0), "loaded sibling On command");
    }

    #[test]
    fn live_screen_layer_consumes_earlier_tween_before_later_message() {
        let mut compiled = compile("screen-layer-tweens", r#"
local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
LEFT = {}
return Def.ActorFrame { OnCommand = function(self)
    underlay:smooth(1.6):diffusealpha(0)
    local updates = 0
    self:SetUpdateFunction(function(self, delta)
        if delta <= 0 then return end
        updates = updates + 1
        if updates == 2 then MESSAGEMAN:Broadcast('StatsShow') end
        LEFT[#LEFT + 1] = underlay:GetTweenTimeLeft()
    end)
end, StatsShowMessageCommand = function(self)
    underlay:smooth(1.6):diffusealpha(1)
end }
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        finish_note_field_intro_fixture(&scene);
        for seconds in [0.8, 1.8, 2.8] {
            let previous = if seconds == 0.8 { 0.0 } else { seconds - 1.0 };
            let output = scene.advance(SongLuaLiveFrame { screen_seconds: seconds,
                delta_seconds: seconds - previous, ..Default::default() }).expect("screen frame");
            assert!(output.errors.is_empty(), "{:?}", output.errors);
        }
        let left = scene.lua.globals().get::<Table>("LEFT").expect("authored tween queries");
        for (index, expected) in [(1, 0.8), (2, 1.6), (3, 0.6)] {
            let actual = left.raw_get::<f32>(index).expect("tween left");
            assert!((actual - expected).abs() < 1e-6, "frame {index}: {actual} != {expected}");
        }
    }

    #[test]
    fn live_gameplay_lead_in_metrics_evaluate_current_theme_rate() {
        let compiled = compile_with_context("gameplay-lead-in-metrics", r#"
local MusicLengthSeconds = GAMESTATE:GetCurrentSong():MusicLengthSeconds()
local MinSecondsToStep = THEME:GetMetric('ScreenGameplay', 'MinSecondsToStep')
local MinSecondsToMusic = THEME:GetMetric('ScreenGameplay', 'MinSecondsToMusic')
local TotalSeconds = MusicLengthSeconds + math.max(MinSecondsToStep, MinSecondsToMusic)
assert(TotalSeconds == 69, 'Numujika lead-in calculation with 1.5x theme rate')
GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(2.5)
SL.Global.ActiveModifiers.MusicRate = 1.03
assert(THEME:GetMetric('ScreenGameplay', 'MinSecondsToStep') == 6.18)
assert(THEME:GetMetric('ScreenGameplay', 'MinSecondsToMusic') == 2.06)
assert(THEME:GetMetric('ScreenGameplay', 'MinSecondsToStepNextSong') == 4.12)
local names = {}
for _, name in ipairs(THEME:GetMetricNamesInGroup('ScreenGameplay')) do names[name] = true end
SL.Global.ActiveModifiers.MusicRate = nil
for _, name in ipairs({'MinSecondsToStep', 'MinSecondsToMusic', 'MinSecondsToStepNextSong'}) do
    assert(THEME:HasMetric('ScreenGameplay', name) and names[name])
    assert(not pcall(function() return THEME:GetMetric('ScreenGameplay', name) end))
end
SL.Global.ActiveModifiers.MusicRate = 0
assert(THEME:GetMetric('ScreenGameplay', 'MinSecondsToStep') == 0)
SL.Global.ActiveModifiers.MusicRate = 1.5
return Def.Actor{}
"#, |context| context.song_music_rate = 1.5);
        assert!(compiled.live_scene.is_some());
    }

    #[test]
    fn live_chart_attacks_use_strict_boundaries_and_keep_song_writes_between_them() {
        let mut compiled = compile_with_context("attack-boundaries", r#"
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(9999)
        STATE = GAMESTATE:GetPlayerState(PLAYER_1)
        STATE:SetPlayerOptions('ModsLevel_Stage', '25% Dark')
        STATE:SetPlayerOptions('ModsLevel_Song', '75% Drunk')
        OPTIONS = STATE:GetPlayerOptions('ModsLevel_Song')
    end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.players[0].enabled = true;
            context.players[1].enabled = false;
        });
        let mut scene = compiled.live_scene.take().unwrap();
        let windows: Arc<[deadsync_gameplay::ChartAttackWindow]> = Arc::from([
            deadsync_gameplay::ChartAttackWindow { start_second: 2.0, len_seconds: 1.0,
                mods: "Overhead, Onlyonecouples".into() },
            deadsync_gameplay::ChartAttackWindow { start_second: 2.5, len_seconds: 1.5,
                mods: "25% Drunk".into() },
        ]);
        let schedule = [windows, Arc::from([])];
        let mut previous = 0.0;
        for (seconds, drunk) in [(0.0, 0.75), (2.0, 0.75), (2.25, 0.0),
            (2.5, 0.5), (2.75, 0.25), (3.0, 0.25), (4.0, 0.0)] {
            // Native attack activation reads the preceding ScreenGameplay
            // position. Prime that input while keeping all boundary assertions.
            scene.advance(SongLuaLiveFrame { music_seconds: seconds,
                screen_seconds: seconds, beat: seconds * 2.0,
                ..Default::default() }).unwrap();
            let output = scene.advance(SongLuaLiveFrame {
                music_seconds: seconds, screen_seconds: seconds, beat: seconds * 2.0,
                delta_seconds: seconds - previous, chart_attacks: Some(schedule.clone()),
                ..Default::default()
            }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            previous = seconds;
            scene.lua.load(format!(r#"
assert(STATE:GetPlayerOptions('ModsLevel_Song') == OPTIONS)
assert(OPTIONS:Drunk() == {drunk}, 'attack targets at {seconds}')
"#)).exec().unwrap();
            if seconds == 2.25 {
                scene.lua.load("OPTIONS:Drunk(0.5)").exec().unwrap();
            }
        }
        scene.lua.load("assert(OPTIONS:Dark() == 0.25)").exec().unwrap();
    }

    #[test]
    fn live_column_cues_derive_owned_tweens_from_chart_notes() {
        let compiled = compile_with_context("column-cue-owned-tweens", "return Def.Actor{}", |context| {
            context.players[0].column_cues = true;
            context.players[1].enabled = false;
            context.song_steps = vec![crate::SongLuaStep {
                steps_type: "StepsType_Dance_Single".into(), difficulty: SongLuaDifficulty::Challenge,
                description: String::new(), chart_name: String::new(), credit: String::new(),
                meter: 9, music_file: String::new(), possible_grade_points: 0,
            }];
            context.player_step_indices = [Some(0), None];
            // Real payloads minimize row divisions. Four stored rows can
            // span twelve beats; their indices are not native 48ths.
            context.song_step_notes = vec![Arc::from(&b"1000\n0M00\n00F0\n000L\n"[..])];
            context.song_step_row_beats = vec![Arc::from([0.0, 4.0, 8.0, 12.0])];
        });
        let mut scene = compiled.live_scene.expect("cue scene");
        finish_note_field_intro_fixture(&scene);
        scene.lua.load(r#"
local steps = GAMESTATE:GetCurrentSteps(PLAYER_1)
local cues = steps:GetColumnCues()
assert(#cues == 4 and cues[1].startTime == 0 and cues[2].duration == 4)
assert(cues[2].columns[1].colNum == 2 and cues[2].columns[1].noteType == 4)
assert(#cues[3].columns == 0 and cues[4].columns[1].noteType == 5)
assert(#steps:GetColumnCues(4) == 2, 'duration threshold is strict; previous zero is special')
ParseColumnCues(steps, 'P1')
assert(SL.P1.Streams.ColumnCues[2].columns[1].isMine)
assert(steps:GetColumnCues()[2].columns[1].noteType == 4, 'fresh mutable query results')
UNDERLAY = SCREENMAN:GetTopScreen():GetChild('Underlay')
CONTAINER = UNDERLAY:GetChild('NoteFieldContainerP1')
CUES = CONTAINER:GetChild('')[2] -- native duplicate-name group: ColumnFlash, ColumnCues, DisplayMods
FLASH = CUES:GetChild('Column1'):GetChild('ColumnFlash')
assert(FLASH:GetParent():GetParent() == CUES and CUES:GetParent() == CONTAINER)
"#).exec().expect("native cue getter, parser and ownership");
        for (screen, music, delta, expected) in [(0.1, -1.0, 0.1, 1.0), (0.3, -0.8, 0.2, 0.8)] {
            let output = scene.advance(SongLuaLiveFrame {
                screen_seconds: screen, music_seconds: music, delta_seconds: delta,
                ..Default::default()
            }).expect("cue frame");
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let globals = scene.lua.globals();
            let flash = globals.get::<Table>("FLASH").unwrap();
            let underlay = globals.get::<Table>("UNDERLAY").unwrap();
            let remaining = crate::actor_tween_time_left(&scene.lua, &flash).unwrap();
            assert!((remaining - expected).abs() < 1e-6, "{remaining} != {expected}");
            assert_eq!(crate::actor_tween_time_left(&scene.lua, &underlay).unwrap(), remaining);
        }
    }

    #[test]
    fn live_lyric_display_loads_text_layers_font_and_changed_commands() {
        let compiled = compile("lyric-display-construction", r#"
return Def.ActorFrame {
    OnCommand = function()
        local screen = SCREENMAN:GetTopScreen()
        local lyrics = screen:GetChild('LyricDisplay')
        assert(lyrics:GetNumChildren() == 2, 'LyricDisplay must load both native text layers')
        LYRIC_DISPLAY = lyrics
        local back, front = lyrics:GetChild('LyricBack'), lyrics:GetChild('LyricFront')
        assert(back:GetParent() == lyrics and front:GetParent() == lyrics)
        assert(lyrics:GetParent() == screen)
        assert(back:GetName() == 'LyricBack' and front:GetName() == 'LyricFront')
        assert(back:GetText() == '' and front:GetText() == '')
        assert(lyrics:GetX() == SCREEN_CENTER_X and lyrics:GetY() == SCREEN_CENTER_Y + 160)
        for _, position in ipairs{{'SetReverse', -140}, {'SetOneReverse', -205}, {'SetNoReverse', 160}} do
            lyrics:playcommand(position[1])
            assert(lyrics:GetX() == SCREEN_CENTER_X and lyrics:GetY() == SCREEN_CENTER_Y + position[2])
        end
        local original_var = Var
        Var = function(name)
            if name == 'LyricText' then return 'fixture lyric' end
            if name == 'LyricDuration' then return 4 end
            if name == 'LyricColor' then return {0.8, 0.6, 0.4, 1} end
            return original_var(name)
        end
        for index, text in ipairs{back, front} do
            text:playcommand('Changed')
            assert(text:GetText() == 'fixture lyric')
            assert(math.abs(text:GetTweenTimeLeft() - 4.4) < 0.00001)
            local factor = index == 1 and 0.5 or 0.9
            local color = text:GetDiffuse()
            for channel, value in ipairs{0.8, 0.6, 0.4} do
                assert(math.abs(color[channel] - value * factor) < 0.00001)
            end
            text:playcommand('Changed')
            assert(math.abs(text:GetTweenTimeLeft() - 4.4) < 0.00001,
                'a changed lyric must replace the previous lyric tween')
        end
        Var = original_var
    end,
}
"#);
        let scene = compiled.live_scene.expect("lyric scene");
        let lyrics = scene.lua.globals().get::<Table>("LYRIC_DISPLAY").unwrap();
        let children = crate::lua_util::actor_direct_children(&scene.lua, &lyrics).unwrap();
        assert_eq!(children.iter().map(|child| crate::lua_util::actor_name(child).unwrap())
            .collect::<Vec<_>>(), ["LyricBack", "LyricFront"]);
        for child in children {
            let (font, path) = crate::lua_util::read_bitmap_font(&child).unwrap()
                .expect("inherited LyricDisplay text font");
            assert_eq!(font, "miso");
            assert!(path.ends_with("assets/fonts/miso/_miso light.ini"));
        }
    }

    #[test]
    fn live_percentage_display_inherits_actor_frame_class_methods() {
        compile("percentage-display-inheritance", r#"
return Def.ActorFrame {
    OnCommand = function()
        local screen = SCREENMAN:GetTopScreen()
        local first = screen:GetChild('ScoreP1'):GetChild('ScoreDisplayPercentage Percent')
        local second = screen:GetChild('ScoreP2'):GetChild('ScoreDisplayPercentage Percent')
        local native_count = ActorFrame.GetNumChildren
        local native_children = ActorFrame.GetChildren
        local counts, children = 0, 0
        ActorFrame.GetNumChildren = function(self)
            if self == first or self == second then counts = counts + 1 end
            return native_count(self)
        end
        ActorFrame.GetChildren = function(self)
            if self == first or self == second then children = children + 1 end
            return native_children(self)
        end
        for index, percent in ipairs{first, second} do
            assert(percent:GetNumChildren() == 1)
            assert(percent:GetChildren()['PercentP' .. index] == percent:GetChild('PercentP' .. index))
        end
        assert(counts == 2 and children == 2,
            'PercentageDisplay must inherit ActorFrame class method overrides')
        ActorFrame.GetNumChildren = native_count
        ActorFrame.GetChildren = native_children
        assert(first.SetUpdateFunction == ActorFrame.SetUpdateFunction)
        first:SetUpdateFunction(function() end)
        first:SetUpdateFunction(nil)
    end,
}
"#);
    }

    #[test]
    fn live_score_display_loads_unnamed_blank_frame_before_percent() {
        compile("score-display-frame", r#"
return Def.ActorFrame {
    OnCommand = function()
        local screen = SCREENMAN:GetTopScreen()
        for _, player in ipairs{'P1', 'P2'} do
            local score = screen:GetChild('Score' .. player)
            assert(score:GetNumChildren() == 2, 'ScoreDisplayPercentage must include its blank frame')
            local frame = assert(score:GetChild(''))
            local percent = assert(score:GetChild('ScoreDisplayPercentage Percent'))
            assert(frame:GetName() == '' and frame:GetParent() == score)
            assert(percent:GetParent() == score)
            assert(score:GetChild('') == frame and score:GetChildren()[''] == frame)
            local texture = assert(frame:GetTexture(), 'score frame must load its inherited bitmap')
            assert(texture:GetPath() == 'Themes/_fallback/Graphics/_blank.png')
            assert(texture:GetSourceWidth() == 4 and texture:GetSourceHeight() == 2)
            assert(texture:GetImageWidth() == 8 and texture:GetImageHeight() == 8)
            assert(texture:GetTextureWidth() == 8 and texture:GetTextureHeight() == 8)
            assert(texture:GetNumFrames() == 1)
            assert(frame:GetWidth() == 4 and frame:GetHeight() == 2)
            assert(percent:GetChild('Percent' .. player):GetParent() == percent)
        end
    end,
}
"#);
    }

    #[test]
    fn live_oni_game_over_loads_inherited_blank_bitmap() {
        compile("oni-game-over-bitmap", r#"
local sprite
local function check(texture)
    assert(texture, 'OniGameOver must load the inherited blank bitmap')
    assert(texture:GetPath() == 'Themes/_fallback/Graphics/_blank.png')
    assert(texture:GetSourceWidth() == 4 and texture:GetSourceHeight() == 2)
    assert(texture:GetImageWidth() == 8 and texture:GetImageHeight() == 8)
    assert(texture:GetTextureWidth() == 8 and texture:GetTextureHeight() == 8)
    assert(texture:GetNumFrames() == 1)
end
return Def.ActorFrame {
    OnCommand = function()
        local screen = SCREENMAN:GetTopScreen()
        local first = screen:GetChild('OniGameOverP1')
        local second = screen:GetChild('OniGameOverP2')
        assert(first:GetName() == 'OniGameOverP1' and first:GetParent() == screen)
        first:zoomto(320, 180)
        local retained = first:GetTexture()
        check(retained)
        check(second:GetTexture())
        assert(sprite:GetTexture() == nil, 'an unloaded Sprite must remain unloaded')
        sprite:SetTexture(retained)
        check(sprite:GetTexture())
        assert(sprite:GetWidth() == 4 and sprite:GetHeight() == 2)
        sprite:Load(nil)
        assert(sprite:GetTexture() == nil)
        second:Load(nil)
        assert(second:GetTexture() == nil)
        second:SetTexture(retained)
        check(second:GetTexture())
    end,
    Def.Sprite { InitCommand = function(self) sprite = self end },
}
"#);
        let key = "Themes/_fallback/Graphics/_blank.png";
        let bitmap = deadlib_assets::generated_texture(key).expect("actual prepared blank bitmap");
        assert_eq!(bitmap.image.dimensions(), (8, 8));
        assert!(bitmap.image.pixels().all(|pixel| pixel.0 == [0, 0, 0, 0]));
        let source = deadlib_assets::texture_dims(key).expect("native bitmap source dimensions");
        assert_eq!((source.w, source.h), (4, 2));
    }

    #[test]
    fn live_large_scene_preserves_actor_identity_and_update_order() {
        let mut compiled = compile("canonical-large-scene", "return Def.ActorFrame {}");
        let base = compiled.live_scene.take().expect("base live scene");
        let lua = base.lua.clone();
        let context = base.context.clone();
        drop(base);
        let declaration: Table = lua.load(r#"
local root = { Class = 'ActorFrame' }
for i = 1, 2800 do
    root[i] = { Class = 'Quad', Name = 'Child' .. i }
end
return root
"#).eval().expect("declare large actor tree");
        // Use the gameplay host's materialization path, rather than invoking
        // Def's declaration decorators outside their source-loading phase.
        let Value::Table(root) = crate::lua_util::own_actor_root(
            &lua, Value::Table(declaration), |_, _| Ok(()),
        ).expect("materialize large actor tree") else { panic!("actor root") };
        lua.globals().set("LARGE_ROOT", root.clone()).unwrap();
        let actors = root.sequence_values::<Table>()
            .collect::<mlua::Result<Vec<_>>>().expect("retain authored actor handles");
        // Compilation registers each initialized actor before retaining the
        // live scene. This also creates its native tween replay state.
        crate::lua_util::register_song_lua_actor(&lua, &root).unwrap();
        for actor in &actors {
            crate::lua_util::register_song_lua_actor(&lua, actor).unwrap();
        }
        let roots = lua.create_sequence_from([root]).unwrap();
        crate::lua_util::invalidate_compile_update_plan(&lua);
        let mut scene = SongLuaLiveScene::new(lua, Value::Table(roots), context,
            Vec::new(), 0, actors, Vec::new(), Vec::new()).expect("construct large live scene");
        let initial = scene.advance(SongLuaLiveFrame::default())
            .expect("initialize large live scene before installing runtime tweens");
        assert!(initial.errors.is_empty(), "{:?}", initial.errors);
        scene.lua.load(r#"
local count = 2800
local root = LARGE_ROOT
local retained = {}
for i = 1, count do
    local child = root:GetChild('Child' .. i)
    retained[i] = child
    child:x(i):aux(i):linear(1):x(i + 1)
end
root:SetUpdateFunction(function(self, delta)
    if delta <= 0 then return end
    for i = 1, count do
        local child = self:GetChild('Child' .. i)
        assert(child == retained[i], 'live preparation must preserve actor identity')
        assert(child:getaux() == i, 'authored child identity and order must be preserved')
        assert(math.abs(child:GetX() - (i + 0.25)) < 0.001,
            'parent Update must observe every child advanced exactly once: child=' .. i ..
            ' x=' .. child:GetX() .. ' delta=' .. delta .. ' left=' .. child:GetTweenTimeLeft())
    end
end)
"#).exec().expect("install large-scene update assertions");
        let output = scene.advance(SongLuaLiveFrame {
            screen_seconds: 0.25, music_seconds: 0.25, delta_seconds: 0.25,
            ..Default::default()
        }).expect("advance large live scene");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }

    #[test]
    fn live_relative_and_canonical_nested_media_keep_hidden_sprite_slots() {
        let dir = std::path::PathBuf::from(format!(
            "../../target/song-lua-relative-media-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
            .save(dir.join("nested/ball.png"))
            .unwrap();
        std::fs::write(dir.join("entry.lua"), "return LoadActor('nested/actor.lua')").unwrap();
        std::fs::write(dir.join("nested/actor.lua"), "return LoadActor('ball.png') .. {Name='HiddenBall',InitCommand=function(self) self:visible(false) end}").unwrap();
        // Windows catalog scanning canonicalizes roots to \\?\ paths. Exercise
        // both that shipping spelling and a process-relative song root.
        let canonical = std::fs::canonicalize(&dir).unwrap();
        for root in [&dir, &canonical] {
            let mut context = crate::SongLuaCompileContext::new(root, "relative media");
            context.live = true;
            let compiled: Compiled = crate::compile_song_lua_with_default_host(
                &root.join("entry.lua"),
                &context,
                SongLuaNoteskinResolver::default(),
                |_, _, _| Ok(Arc::<[()]>::from([])),
                |_| None,
                |_, _| None,
            )
            .unwrap();
            let sprite = compiled
                .overlays
                .iter()
                .find(|actor| actor.name.as_deref() == Some("HiddenBall"))
                .expect("nested hidden sprite retains a visual slot");
            assert!(matches!(&sprite.kind, SongLuaOverlayKind::Sprite { texture_path, .. }
                if texture_path.is_absolute() && texture_path.is_file()));
        }
    }

    #[test]
    fn live_render_defaults_do_not_dispatch_public_class_lookups() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        actor.raw_set("__songlua_actor_type", "Actor").unwrap();
        actor.raw_set("__songlua_state_x", 42.0).unwrap();
        let metatable = lua.create_table().unwrap();
        metatable
            .raw_set(
                "__index",
                lua.create_function(|_, _: mlua::MultiValue| {
                    Err::<Value, _>(mlua::Error::runtime(
                        "render defaults invoked public class lookup",
                    ))
                })
                .unwrap(),
            )
            .unwrap();
        actor.set_metatable(Some(metatable)).unwrap();
        let state = crate::actor_overlay_initial_state(&actor).unwrap();
        assert_eq!(state.x, 42.0);
        assert!(state.visible);
        assert_eq!(state.diffuse, [1.0; 4]);
    }

    #[test]
    fn live_vertex_upload_copies_values_before_source_table_mutation() {
        let compiled = compile(
            "live-vertex-value-copy",
            r#"
return Def.ActorMultiVertex {
 InitCommand=function(self)
  local vertices = {
   {{1,2,-3},{0.25,0.5,0.75,1},{0.1,0.2}},
   {{4,5,-6},{1,1,1,1},{0,1}},
   {{7,8,-9},{1,1,1,1},{1,0}},
  }
  self:SetDrawState{Mode='DrawMode_Triangles'}:SetVertices(vertices)
  assert(self:GetNumVertices()==3)
  vertices[1][1][1]=999
  vertices[1][2][4]=0
  vertices[1][3][1]=999
 end,
}
"#,
        );
        let vertices = compiled
            .overlays
            .iter()
            .find_map(|actor| match &actor.kind {
                SongLuaOverlayKind::ActorMultiVertex { vertices, .. } => Some(vertices),
                _ => None,
            })
            .expect("uploaded mesh remains drawable");
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[0].pos, [1.0, 2.0]);
        assert_eq!(vertices[0].z, -3.0);
        assert_eq!(vertices[0].color, [0.25, 0.5, 0.75, 1.0]);
        assert_eq!(vertices[0].uv, [0.1, 0.2]);
    }

    #[test]
    fn live_content_reaches_visual_slots_after_input_and_updates() {
        let mut compiled = compile("live-input-visual-content", r#"
local mesh, text, held, x = nil, nil, false, 0
return Def.ActorFrame {
    OnCommand = function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.button == 'Right' then held = event.type ~= 'InputEventType_Release' end
        end)
        self:SetUpdateFunction(function(_, delta)
            if delta <= 0 then return end
            if held then x = x + 1 end
            mesh:SetVertices{
                {{x,0,-10},{1,1,1,1},{0,0}},
                {{x+10,0,-20},{1,1,1,1},{1,0}},
                {{x,10,-30},{1,1,1,1},{0,1}},
                {{0/0,0/0,0/0}}, {{0/0,0/0,0/0}}, {{0/0,0/0,0/0}},
            }
            text:settext(tostring(x))
        end)
    end,
    Def.ActorMultiVertex {
        Name='MovingMesh',
        InitCommand=function(self) mesh=self; self:SetDrawState{Mode='DrawMode_Triangles'}:SetNumVertices(6) end,
    },
    Def.BitmapText { Name='TimerText', Font='Common normal', InitCommand=function(self) text=self end },
}
"#);
        let mesh_local = compiled.overlays.iter().position(|actor| actor.name.as_deref() == Some("MovingMesh"))
            .expect("initially empty native mesh retains a visual slot");
        let text_local = compiled.overlays.iter().position(|actor| actor.name.as_deref() == Some("TimerText")).unwrap();
        let mesh_global = compiled.live_actor_indices()[mesh_local];
        let text_global = compiled.live_actor_indices()[text_local];
        let mut mesh = compiled.overlays[mesh_local].kind.clone();
        let mut text = compiled.overlays[text_local].kind.clone();
        let mut scene = compiled.live_scene.take().unwrap();
        let mut input = crate::SongLuaInputEvent {
            player:Some(0),controller:Some(0),button:"Right",game_button:"MenuRight",
            kind:crate::SongLuaInputType::FirstPress,device:"InputDevice_Key",device_button:"DeviceButton_right",
            level:1.0,z:0,down:true,ago:0.0,is_joystick:false,is_mouse:false,
        };
        assert!(scene.input(&input).unwrap().1.errors.is_empty());
        for step in 1..=2 {
            let seconds = f64::from(step) / 60.0;
            assert!(scene.advance(SongLuaLiveFrame { screen_seconds:seconds,music_seconds:seconds,
                delta_seconds:1.0/60.0,..Default::default() }).unwrap().errors.is_empty());
        }
        sync_visual_content(&scene.actors[mesh_global], &mut mesh).unwrap();
        sync_visual_content(&scene.actors[text_global], &mut text).unwrap();
        let SongLuaOverlayKind::ActorMultiVertex { vertices,texture_path,.. } = &mesh else { panic!("mesh") };
        assert_eq!(vertices.len(),6,"non-finite vertices preserve native triangle boundaries");
        assert_eq!(vertices[0].pos,[2.0,0.0]);
        assert_eq!(vertices.iter().take(3).map(|v| v.z).collect::<Vec<_>>(),[-10.0,-20.0,-30.0]);
        assert!(vertices[3].pos[0].is_nan());
        assert!(texture_path.is_some(),"native default texture is opaque white");
        assert!(matches!(&text,SongLuaOverlayKind::BitmapText{text,..} if text.as_ref()=="2"));
        input.kind=crate::SongLuaInputType::Release;
        input.down=false;
        scene.input(&input).unwrap();
        scene.advance(SongLuaLiveFrame{screen_seconds:0.05,music_seconds:0.05,
            delta_seconds:1.0/60.0,..Default::default()}).unwrap();
        sync_visual_content(&scene.actors[mesh_global], &mut mesh).unwrap();
        let SongLuaOverlayKind::ActorMultiVertex{vertices,..}=&mesh else{panic!("mesh")};
        assert_eq!(vertices[0].pos,[2.0,0.0],"release reaches the authored controller");
    }

    #[test]
    fn live_empty_bitmap_text_zoomtoheight_sets_native_zoom() {
        let mut compiled = compile("empty-text-zoomtoheight", r#"
return Def.BitmapText {
    Font = 'Common normal',
    InitCommand = function(self)
        TEXT = self
        assert(self:GetText() == '' and self:GetHeight() == 1)
        self:zoomtoheight(0.004 * SCREEN_HEIGHT)
        local expected = 1.9199999570846558
        assert(self:GetZoomY() == expected, 'native ZoomToHeight changes zoom before SetText')
        self:zoomx(self:GetZoomY())
        assert(self:GetZoomX() == expected and self:GetZoomedHeight() == expected)
        self:zoomtoheight(3)
        assert(self:GetHeight() == 1 and self:GetZoomY() == 3,
            'repeated sizing divides by the original unzoomed height')
        self:settext('1:59')
        assert(self:GetZoomY() == 3, 'later text assignment preserves the authored zoom')
    end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("live timer text");
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 0.1,
            music_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let text = scene.lua.globals().get::<Table>("TEXT").unwrap();
        let state = read_state(&scene.lua, &text).unwrap();
        assert_eq!(state.zoom_x, 1.92);
        assert_eq!(state.zoom_y, 3.0);
        assert_eq!(state.size, None, "empty text sizing is a zoom, not a future font fit");
    }

    #[test]
    fn live_amv_load_texture_preserves_mesh_and_restores_native_default() {
        let compiled = compile_with_context("amv-load-texture", r#"
return Def.ActorMultiVertex {
    InitCommand = function(self)
        MESH = self
        self:SetDrawState{Mode='DrawMode_Triangles'}:SetVertices{
            {{0,0,0},{1,1,1,1},{0,0}},
            {{20,0,0},{1,1,1,1},{1,0}},
            {{0,10,0},{1,1,1,1},{0,1}},
        }
        assert(self:LoadTexture(GAMESTATE:GetCurrentSong():GetSongDir() .. 'panel.png') == self)
        local texture = self:GetTexture()
        assert(texture:GetSourceWidth() == 16 and texture:GetSourceHeight() == 8)
        assert(self:GetNumVertices() == 3 and self:GetDestDrawMode() == 'DrawMode_Triangles')
    end,
}
"#, |context| {
            image::RgbaImage::new(16, 8).save(context.song_dir.join("panel.png")).unwrap();
        });
        let SongLuaOverlayKind::ActorMultiVertex { vertices, texture_path: Some(path), .. } =
            &compiled.overlays[0].kind else { panic!("textured authored mesh") };
        assert_eq!(path.file_name().unwrap(), "panel.png");
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[1].pos, [20.0, 0.0]);
        assert_eq!(vertices[1].uv, [1.0, 0.0]);
        let scene = compiled.live_scene.expect("live mesh scene");
        scene.lua.load(r#"
local retained = MESH:GetTexture()
assert(MESH:LoadTexture(nil) == MESH)
local default = MESH:GetTexture()
assert(default:GetPath() == '' and default:GetSourceWidth() == 1 and default:GetNumFrames() == 1)
assert(retained:GetSourceWidth() == 16, 'replacement preserves retained texture handles')
assert(MESH:GetNumVertices() == 3 and MESH:GetDestDrawMode() == 'DrawMode_Triangles')
MESH:SetTexture(retained)
assert(MESH:GetTexture() == retained)
MESH:LoadTexture()
assert(MESH:GetTexture():GetSourceWidth() == 1, 'missing argument also loads the default')
"#).exec().expect("native LoadTexture reset and mesh preservation");
    }

    #[test]
    fn live_quad_default_texture_survives_geometry_and_explicit_rebinding() {
        compile("quad-default-texture", r#"
local quad, sprite
local function check(texture)
    assert(texture, 'Quad must load its native default texture')
    assert(texture:GetPath() == '', 'default texture resource has an empty filename')
    for _, method in ipairs{'GetSourceWidth', 'GetSourceHeight', 'GetTextureWidth',
        'GetTextureHeight', 'GetImageWidth', 'GetImageHeight', 'GetNumFrames'} do
        assert(texture[method](texture) == 1, method .. ' must describe the native 1x1 default')
    end
end
return Def.ActorFrame {
    OnCommand = function()
        quad:zoomto(320, 180)
        local original = quad:GetTexture()
        check(original)
        assert(sprite:GetTexture() == nil, 'an unloaded Sprite has no texture')
        sprite:SetTexture(original)
        check(sprite:GetTexture())
        quad:Load(nil)
        assert(quad:GetTexture() == nil, 'Load(nil) unloads the current Quad texture')
        quad:SetTexture(original)
        check(quad:GetTexture())
    end,
    Def.Quad { InitCommand = function(self) quad = self end },
    Def.Sprite { InitCommand = function(self) sprite = self end },
}
"#);
        compile("quad-ignores-node-texture", r#"
return Def.Quad {
    Texture = 'ignored 4x2.png',
    OnCommand = function(self)
        local texture = assert(self:GetTexture(), 'Quad default exists despite a Texture node attribute')
        assert(texture:GetPath() == '' and texture:GetNumFrames() == 1)
        assert(texture:GetSourceWidth() == 1 and texture:GetSourceHeight() == 1)
    end,
}
"#);
    }

    #[test]
    fn live_cancel_transition_loads_quad_and_native_fade() {
        let mut compiled = compile("cancel-transition-child", r#"
return Def.Actor { OnCommand = function(self) self:sleep(9999) end }
"#);
        let mut scene = compiled.live_scene.take().expect("cancel transition scene");
        let screen: Table = scene.lua.globals().get("__songlua_top_screen").unwrap();
        let cancel = crate::actor_children(&scene.lua, &screen).unwrap()
            .raw_get::<Table>("Cancel").unwrap();
        assert_eq!(cancel.get::<Function>("GetNumChildren").unwrap()
            .call::<usize>(cancel.clone()).unwrap(), 1,
            "Cancel must retain its native loaded transition child");
        let children = cancel.get::<Function>("GetChildren").unwrap()
            .call::<Table>(cancel.clone()).unwrap();
        let quad = children.raw_get::<Table>("").expect("unnamed cancel Quad");
        assert_eq!(quad.get::<Function>("GetParent").unwrap()
            .call::<Table>(quad.clone()).unwrap(), cancel);
        assert_eq!(quad.get::<Function>("GetZoomedWidth").unwrap()
            .call::<f32>(quad.clone()).unwrap(), scene.context.screen_width);
        assert_eq!(quad.get::<Function>("GetZoomedHeight").unwrap()
            .call::<f32>(quad.clone()).unwrap(), scene.context.screen_height);
        assert_eq!(read_state(&scene.lua, &quad).unwrap().diffuse[3], 0.0);
        scene.advance(SongLuaLiveFrame { screen_seconds: 0.25, delta_seconds: 0.25,
            ..Default::default() }).unwrap();
        assert_eq!(crate::actor_tween_time_left(&scene.lua, &cancel).unwrap(), 0.0);
        crate::lua_util::start_actor_transition(&scene.lua, &cancel).unwrap();
        assert!((crate::actor_tween_time_left(&scene.lua, &cancel).unwrap() - 0.5).abs() < 1e-6);
        for (seconds, delta, alpha) in [(0.35, 0.1, 0.0), (0.55, 0.2, 0.5), (0.75, 0.2, 1.0)] {
            let output = scene.advance(SongLuaLiveFrame { screen_seconds: seconds,
                delta_seconds: delta, ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            assert!((read_state(&scene.lua, &quad).unwrap().diffuse[3] - alpha).abs() < 1e-5);
        }
        scene.advance(SongLuaLiveFrame { screen_seconds: 0.76, delta_seconds: 0.01,
            ..Default::default() }).unwrap();
        assert_eq!(cancel.raw_get::<u8>("__songlua_transition_state").unwrap(), 2);
    }

    #[test]
    fn live_banner_uses_declared_artwork_and_native_fallback() {
        for (name, selected) in [("declared", true), ("fallback", false)] {
            let source = r#"
local song = GAMESTATE:GetCurrentSong()
local expected = $SELECTED
local function check(banner)
    local texture = assert(banner:GetTexture(), 'Banner must load a real selected or fallback texture')
    if expected then
        assert(texture:GetPath():find('chosen artwork.png', 1, true))
        assert(texture:GetSourceWidth() == 16 and texture:GetSourceHeight() == 8)
    else
        assert(texture:GetPath():find('Themes/_fallback/Graphics/_blank.png', 1, true))
        assert(texture:GetSourceWidth() == 4 and texture:GetSourceHeight() == 2)
    end
end
return Def.Banner {
    OnCommand=function(self)
        if expected then
            assert(song:HasBanner())
            assert(song:GetBannerPath():find('chosen artwork.png', 1, true),
                'Song banner API must use the parsed declared artwork')
        else assert(not song:HasBanner()) end
        self:SetScrolling(true, 0.25):LoadFromSong(song)
        check(self)
        assert(not self:GetScrolling())
        local pane = SCREENMAN:GetTopScreen():GetChild('Underlay'):GetChild('StepStatsPaneP1')
        check(pane:GetChild('BannerAndData'):GetChild('')[1])
        self:SetScrolling(true, 0.5):LoadFromSong(nil)
        local fallback = assert(self:GetTexture())
        assert(fallback:GetPath():find('Themes/_fallback/Graphics/_blank.png', 1, true))
        assert(fallback:GetSourceWidth() == 4 and fallback:GetSourceHeight() == 2)
        assert(not self:GetScrolling())
    end,
}
"#.replace("$SELECTED", if selected { "true" } else { "false" });
            let compiled = compile_with_context(&format!("declared-banner-{name}"), &source, |context| {
                context.screen_width = 854.0;
                if selected {
                    let art = context.song_dir.join("art");
                    fs::create_dir_all(&art).unwrap();
                    let declared = art.join("chosen artwork.png");
                    image::RgbaImage::from_pixel(16, 8, image::Rgba([11, 22, 33, 255]))
                        .save(&declared).unwrap();
                    image::RgbaImage::from_pixel(8, 8, image::Rgba([99, 88, 77, 255]))
                        .save(context.song_dir.join("banner.png")).unwrap();
                    context.song_banner_path = Some(declared);
                }
            });
            assert!(compiled.live_scene.is_some(), "banner case must retain its real live scene");
        }
    }



    #[test]
    fn live_note_field_container_owns_original_children_and_mod_lifecycle() {
        for player in 0..2 {
            let mut compiled=compile_with_context(&format!("native-note-field-container-{player}"),
                "return Def.ActorFrame {}", |context| {
                    context.defer_initial_live_update=true;
                    context.screen_width=854.0;
                    for side in 0..2 {
                        context.players[side].enabled=side==player;
                        context.players[side].column_cues=true;
                    }
                    context.song_steps=vec![crate::SongLuaStep {
                        steps_type:"StepsType_Dance_Single".into(),difficulty:SongLuaDifficulty::Challenge,
                        description:String::new(),chart_name:String::new(),credit:String::new(),
                        meter:9,music_file:String::new(),possible_grade_points:0 }];
                    context.player_step_indices=if player==0 {[Some(0),None]} else {[None,Some(0)]};
                    context.song_step_notes=vec![Arc::from(&b"1000\n0M00\n00F0\n000L\n"[..])];
                    context.song_step_row_beats=vec![Arc::from([0.0,4.0,8.0,12.0])];
                });
            let mut scene=compiled.live_scene.take().expect("native container scene");
            scene.lua.globals().set("PN",format!("PlayerNumber_P{}",player+1)).unwrap();
            scene.lua.globals().set("SIDE",player+1).unwrap();
            scene.lua.load(r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
local container=underlay:GetChild('NoteFieldContainerP'..SIDE)
assert(container:GetNumChildren()==3, 'native NoteField container must own its three enabled children')
assert(container:GetParent()==underlay)
local children=container:GetChild('')
assert(#children==3)
local flash,cues,mods=children[1],children[2],children[3]
assert(flash:GetParent()==container and flash:GetNumChildren()==4)
assert(cues:GetParent()==container and cues:GetNumChildren()==4)
assert(mods:GetParent()==container and mods:GetNumChildren()==1)
assert(math.abs(mods:GetTweenTimeLeft()-5.5)<0.00001, 'real DisplayMods On queue is retained')
for _,quad in ipairs(flash:GetChild('')) do
    assert(quad:GetParent()==flash and quad:GetTexture())
end
assert(cues:GetChild('Column1'):GetChild('ColumnFlash'):GetParent():GetParent()==cues)
MODS=mods
MODTEXT=mods:GetChild('')
MODTEXT:settext('stale')
MESSAGEMAN:Broadcast('PlayerOptionsChanged',{Player=PN})
assert(MODTEXT:GetText()~='stale', 'native options message refreshes the actual display text')
"#).exec().unwrap();
            let output=scene.advance(SongLuaLiveFrame { screen_seconds:0.001,delta_seconds:0.001,
                music_seconds:0.001,beat:0.002,..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}",output.errors);
            scene.lua.load("assert(math.abs(MODS:GetTweenTimeLeft()-5.499)<0.00001, 'native options message restarts On through the ordinary queue')").exec().unwrap();
        }
    }

    #[test]
    fn live_song_meter_owns_native_song_info_children_and_lifecycle() {
        for (name,width,height) in [("wide",854.0,480.0),("standard",640.0,480.0)] {
            let mut compiled=compile_with_context(&format!("song-meter-{name}"),
                "return Def.ActorFrame {}", |context| {
                    context.main_title="Native Song Info".into();
                    context.screen_width=width;
                    context.screen_height=height;
                    context.defer_initial_live_update=true;
                });
            let scene=compiled.live_scene.as_mut().expect("retained SongMeter scene");
            scene.lua.load(r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
local meter=underlay:GetChild('SongMeter')
assert(meter and meter:GetNumChildren()==4, 'native SongInfoBar must own four direct children')
assert(meter:GetParent()==underlay)
local anonymous=meter:GetChild('')
assert(#anonymous==3)
for i=1,2 do
    assert(anonymous[i]:GetParent()==meter and anonymous[i]:GetTexture(), 'border Quad owns native default texture')
    assert(anonymous[i]:GetTexture():GetSourceWidth()==1 and anonymous[i]:GetTexture():GetSourceHeight()==1)
end
assert(tostring(anonymous[3]):find('SongMeterDisplay',1,true))
assert(anonymous[3]:GetParent()==meter)
local title=meter:GetChild('SongTitle')
assert(title and title:GetParent()==meter and title:GetText()=='Native Song Info')
title:settext('stale')
MESSAGEMAN:Broadcast('CurrentSongChanged')
assert(title:GetText()=='Native Song Info', 'normal song change updates the real native title actor')
"#).exec().unwrap();
        }
    }

    #[test]
    fn live_theme_setup_broadcasts_respect_song_construction_boundary() {
        use deadsync_rules::timing::{TimingData, TimingSegments};
        for reload in [false, true] {
            let source=format!(r#"
ROUND=(ROUND or 0)+1
return Def.ActorFrame {{
    InitCommand=function(self)
        assert(GAMESTATE:Env().P1PeakNPS==1.5, 'real theme histogram must initialize before song Init')
    end,
    OnCommand=function(self)
        MESSAGEMAN:Broadcast('PeakNPSUpdated')
        self:sleep(0.25):queuecommand('AuthoredRuntime')
        if {reload} and ROUND==1 then SCREENMAN:SetNewScreen('ScreenGameplay') end
    end,
    PeakNPSUpdatedMessageCommand=function(self)
        MESSAGEMAN:Broadcast('AuthoredPeakReaction')
    end,
    AuthoredRuntimeCommand=function(self)
        MESSAGEMAN:Broadcast('CurrentSongChanged')
        MESSAGEMAN:Broadcast('PeakNPSUpdated')
    end,
}}
"#);
            let mut compiled=compile_with_context(&format!("theme-broadcast-boundary-{reload}"), &source, |context| {
                context.defer_initial_live_update=true;
                context.screen_width=854.0;
                context.players[0].enabled=true;
                context.players[1].enabled=false;
                context.players[0].step_statistics_enabled=true;
                context.player_step_indices=[Some(0), None];
                context.song_steps=vec![crate::SongLuaStep { steps_type:"StepsType_Dance_Single".into(),
                    difficulty:crate::SongLuaDifficulty::Challenge, description:String::new(),chart_name:String::new(),
                    credit:String::new(),meter:12,music_file:String::new(),possible_grade_points:100 }];
                let timing=TimingData::from_segments(0.0,0.0,&TimingSegments {
                    bpms:vec![(0.0,120.0)],..Default::default()},&[]);
                context.song_step_timing=vec![timing.clone()];
                context.player_timing=[Some(timing),None];
                context.song_step_notes=vec![std::sync::Arc::from(&b"1000\n0100\n2000\n3000\nM000\n4000\n3000\n1000\n"[..])];
                context.song_step_row_beats=vec![std::sync::Arc::from(&[0.0,1.0,2.0,3.0,4.0,5.0,6.0,7.0][..])];
            });
            assert_eq!(compiled.startup_reloads, u32::from(reload));
            let mut scene=compiled.live_scene.take().expect("retained boundary scene");
            let startup=scene.take_startup_output();
            assert!(startup.errors.is_empty(), "{:?}", startup.errors);
            // compile.rs resets source capture when reconstructing a startup scene;
            // take_startup_output exposes the retained scene's source output. The
            // legacy stream separately keeps broadcasts from prior startup scenes.
            assert_eq!(scene.lua.globals().get::<u32>("ROUND").unwrap(), if reload {2} else {1});
            assert_eq!(startup.broadcasts.iter().filter(|(_, name, _)| name=="PeakNPSUpdated").count(),
                if reload {2} else {1});
            let expected=["PeakNPSUpdated", "AuthoredPeakReaction"];
            assert_eq!(startup.source_broadcasts.iter().map(|event|event.name.as_str()).collect::<Vec<_>>(),
                expected, "pre-song theme broadcasts must stay outside authored song output");
            scene.advance(SongLuaLiveFrame::default()).unwrap();
            let output=scene.advance(SongLuaLiveFrame { screen_seconds:0.5,delta_seconds:0.5,
                music_seconds:0.5,beat:1.0,..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            assert_eq!(output.source_broadcasts.iter().map(|event|event.name.as_str()).collect::<Vec<_>>(),
                ["CurrentSongChanged", "PeakNPSUpdated", "AuthoredPeakReaction", "PeakNPSUpdated", "AuthoredPeakReaction"],
                "theme broadcasts nested under authored runtime commands must remain observable");
        }
    }

    #[test]
    fn live_danger_owns_native_quad_and_health_lifecycle() {
        for (name, enabled, double, hidden, hide_danger) in [
            ("p1",[true,false],false,false,false),
            ("p2",[false,true],false,false,false),
            ("double",[true,false],true,false,false),
            ("hidden-life",[true,false],false,true,false),
            ("hide-danger",[true,false],false,false,true),
        ] {
            let mut compiled=compile_with_context(&format!("native-danger-{name}"),
                "return Def.ActorFrame {}", |context| {
                    context.screen_width=854.0;
                    context.defer_initial_live_update=true;
                    if double { context.style_name="double".into(); }
                    for player in 0..2 {
                        context.players[player].enabled=enabled[player];
                        context.players[player].hide_lifebar=hidden;
                        context.players[player].hide_danger=hide_danger;
                    }
                });
            let mut scene=compiled.live_scene.take().expect("retained Danger scene");
            scene.lua.globals().set("DOUBLE",double).unwrap();
            scene.lua.globals().set("HIDDEN",hidden).unwrap();
            scene.lua.globals().set("HIDE_DANGER",hide_danger).unwrap();
            scene.lua.load(r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
for p=1,2 do
    local actor=underlay:GetChild('DangerP'..p)
    if GAMESTATE:IsPlayerEnabled('PlayerNumber_P'..p) then
        assert(actor and actor:GetTexture(), 'Danger must own the native Quad default texture')
        assert(actor:GetTexture():GetSourceWidth()==1 and actor:GetTexture():GetSourceHeight()==1)
        assert(actor:GetParent()==underlay and actor:GetVisible()==not HIDDEN)
        assert(actor:GetZoomedWidth()==(DOUBLE and 854 or 427) and actor:GetZoomedHeight()==480)
        assert(actor:GetX()==(DOUBLE and 427 or (p==1 and 213.5 or 640.5)) and actor:GetY()==240)
        assert(actor:GetDiffuseAlpha()==0)
        assert(SL['P'..p].ActiveModifiers.HideDanger==HIDE_DANGER)
        DANGER=actor; PN='PlayerNumber_P'..p
    else assert(actor==nil, 'inactive player has no Danger actor') end
end
"#).exec().unwrap();
            scene.advance(SongLuaLiveFrame::default()).unwrap();
            scene.advance(SongLuaLiveFrame { screen_seconds:0.3,delta_seconds:0.3,
                music_seconds:0.3,beat:0.6,..Default::default() }).unwrap();
            scene.lua.load(r#"
MESSAGEMAN:Broadcast('HealthStateChanged',{PlayerNumber=PN,HealthState='HealthState_Danger'})
assert(math.abs(DANGER:GetTweenTimeLeft()-(HIDE_DANGER and 0 or 0.3))<0.00001)
"#).exec().unwrap();
            scene.advance(SongLuaLiveFrame { screen_seconds:0.6,delta_seconds:0.3,
                music_seconds:0.6,beat:1.2,..Default::default() }).unwrap();
            scene.lua.load(r#"
assert(math.abs(DANGER:GetDiffuseAlpha()-(HIDE_DANGER and 0 or 0.7))<0.00001)
MESSAGEMAN:Broadcast('HealthStateChanged',{PlayerNumber=PN,HealthState='HealthState_Dead'})
assert(math.abs(DANGER:GetTweenTimeLeft()-0.6)<0.00001, 'fail flash survives HideDanger')
"#).exec().unwrap();
            scene.advance(SongLuaLiveFrame { screen_seconds:0.9,delta_seconds:0.3,
                music_seconds:0.9,beat:1.8,..Default::default() }).unwrap();
            scene.lua.load("assert(DANGER:GetDiffuseAlpha()==0 and math.abs(DANGER:GetTweenTimeLeft()-0.3)<0.00001, 'native diffuse getter reports queued destination while the fail flash still has a pending tween')").exec().unwrap();
            scene.advance(SongLuaLiveFrame { screen_seconds:1.2,delta_seconds:0.3,
                music_seconds:1.2,beat:2.4,..Default::default() }).unwrap();
            scene.lua.load("assert(DANGER:GetDiffuseAlpha()==0 and DANGER:GetTweenTimeLeft()==0)").exec().unwrap();
        }
        for (name, setup) in [
            ("casual", "SL.Global.GameMode='Casual'"),
            ("fail-off", "GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Preferred'):FailSetting('FailType_Off')"),
        ] {
            let source=format!(r#"
ROUND=(ROUND or 0)+1
return Def.ActorFrame {{ OnCommand=function(self)
    if ROUND==1 then {setup}; SCREENMAN:SetNewScreen('ScreenGameplay') end
end }}
"#);
            let compiled=compile_with_context(&format!("native-danger-{name}"),&source,|context| {
                context.players[1].enabled=false;
                context.defer_initial_live_update=true;
            });
            assert_eq!(compiled.startup_reloads,1);
            let scene=compiled.live_scene.expect("retained eligibility reload");
            scene.lua.load("assert(SCREENMAN:GetTopScreen():GetChild('Underlay'):GetChild('DangerP1')==nil, 'native Danger eligibility must survive screen reconstruction')").exec().unwrap();
        }
    }

    #[test]
    fn live_step_statistics_judgment_labels_follow_initial_timing_windows() {
        for (name, windows, count) in [
            ("all", [true; 5], 12),
            ("no-way-off", [true, true, true, true, false], 11),
            ("casual", [true, true, true, false, false], 10),
            ("no-top-two", [false, false, true, true, true], 10),
        ] {
            let mut compiled=compile_with_context(&format!("timing-window-labels-{name}"),
                "return Def.ActorFrame {}", |context| {
                    context.screen_width=854.0;
                    context.defer_initial_live_update=true;
                    context.players[0].timing_windows=windows;
                    context.players[1].timing_windows=[true;5];
                });
            let scene=compiled.live_scene.take().expect("real StepStatistics scene");
            scene.lua.globals().set("EXPECTED_COUNT",count).unwrap();
            scene.lua.globals().set("EXPECTED_WINDOWS",scene.lua.create_sequence_from(windows).unwrap()).unwrap();
            scene.lua.load(r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
for player=1,2 do
    local expected=player==1 and EXPECTED_WINDOWS or {true,true,true,true,true}
    local actual=SL['P'..player].ActiveModifiers.TimingWindows
    for i=1,5 do
        assert(actual[i]==expected[i], 'SL must use each player initial timing-window inputs')
    end
    local judgments=underlay:GetChild('StepStatsPaneP'..player):GetChild('BannerAndData'):GetChild('TapNoteJudgments')
    assert(judgments:GetNumChildren()==(player==1 and EXPECTED_COUNT or 12),
        'real native judgment label actors must follow enabled timing windows')
    local children=judgments:GetChild('')
    local labels={}
    local values=0
    for _,child in ipairs(children) do
        local text=child:GetText()
        if text=='0000' then values=values+1 else labels[text]=true end
    end
    assert(values==6, 'all six native judgment counters remain present')
    for i=1,5 do
        local label=THEME:GetString('TapNoteScore','W'..i):upper()
        assert((labels[label] or false)==expected[i], 'enabled judgment label must exist; disabled label must be absent')
    end
    assert(labels[THEME:GetString('TapNoteScore','Miss'):upper()], 'Miss label always exists')
end
"#).exec().unwrap();
        }
    }

    #[test]
    fn live_step_statistics_owns_native_resources_and_updates_real_data() {
        use deadsync_rules::timing::{TimingData, TimingSegments};
        for (name, width, height, enabled, centered, selected) in [
            ("p1",854.0,480.0,[true,false],false,true),
            ("p2-centered",854.0,480.0,[false,true],true,true),
            ("ultrawide",2560.0,720.0,[true,true],false,true),
            ("narrow",640.0,480.0,[true,false],true,true),
            ("disabled",854.0,480.0,[true,false],false,false)] {
            let mut compiled = compile_with_context(&format!("native-step-statistics-{name}"),
                "return Def.ActorFrame {}", |context| {
                    context.defer_initial_live_update=true;
                    context.screen_width=width; context.screen_height=height;
                    for player in 0..2 {
                        context.players[player].enabled=enabled[player];
                        context.players[player].screen_x=if centered {width/2.0} else {320.0};
                        context.players[player].step_statistics_enabled=selected;
                        context.players[player].timing_windows=[true,true,true,false,false];
                        context.players[player].background_filter_percent=75;
                    }
                    context.player_step_indices=[Some(0),Some(0)];
                    context.song_steps=vec![crate::SongLuaStep { steps_type:"StepsType_Dance_Single".into(),
                        difficulty:SongLuaDifficulty::Hard,description:String::new(),chart_name:String::new(),
                        credit:String::new(),meter:12,music_file:String::new(),possible_grade_points:100 }];
                    let timing=TimingData::from_segments(0.0,0.0,&TimingSegments {
                        bpms:vec![(0.0,120.0)],..Default::default()},&[]);
                    context.song_step_timing=vec![timing.clone()];
                    context.player_timing=[Some(timing.clone()),Some(timing)];
                    context.song_step_notes=vec![std::sync::Arc::from(&b"1000\n0100\n2000\n3000\nM000\n4000\n3000\n1000\n"[..])];
                    context.song_step_row_beats=vec![std::sync::Arc::from(&[0.0,1.0,2.0,3.0,4.0,5.0,6.0,7.0][..])];
                });
            let mut scene=compiled.live_scene.take().unwrap();
            scene.lua.globals().set("EXPECT_PANE",selected && width>640.0).unwrap();
            scene.lua.load(r#"
PANES={}
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
for p=1,2 do
    local pane=underlay:GetChild('StepStatsPaneP'..p)
    if GAMESTATE:IsPlayerEnabled('PlayerNumber_P'..p) and EXPECT_PANE then
        assert(pane and pane:GetNumChildren()==3, 'StepStatistics must own its real three native children')
        assert(pane:GetParent()==underlay)
        local data=pane:GetChild('BannerAndData')
        assert(data and data:GetNumChildren()==4)
        assert(data:GetChild('TapNoteJudgments'):GetNumChildren()==10)
        assert(data:GetChild('HoldsMinesRolls'):GetChild('Holds_Values'):GetText()=='000/001')
        assert(data:GetChild('HoldsMinesRolls'):GetChild('Mines_Values'):GetText()=='000/001')
        local loose=data:GetChild('')
        local banner, timer=loose[1],loose[2]
        assert(tostring(banner):find('Banner',1,true) and timer:GetNumChildren()==4)
        local time=timer:GetChild('')
        assert(time[1]:GetText()=='' and time[3]:GetText()=='1:00', 'native pre-update timer: '..time[1]:GetText()..' / '..time[3]:GetText())
        local parts=pane:GetChild('')
        local dark, graph=parts[1],parts[2]
        assert(tostring(dark):find('Sprite',1,true) and dark:GetTexture())
        assert(graph:GetNumChildren()==3)
        local graph_parts=graph:GetChild('')
        local plot=graph_parts[3]
        assert(plot:GetNumChildren()==2 and plot:GetChild('LifeLine_AMV'))
        assert(GAMESTATE:Env()['P'..p..'PeakNPS']==1.5)
        PANES[#PANES+1]={pane=pane, timer=time[1], count=data:GetChild('TapNoteJudgments'):GetChild('')[1],
            life=plot:GetChild('LifeLine_AMV'), histogram=plot:GetChild(''), player='PlayerNumber_P'..p}
    else assert(pane==nil) end
end
"#).exec().unwrap();
            if !selected || width<=640.0 { continue; }
            let output=scene.advance(SongLuaLiveFrame { screen_seconds:0.01,delta_seconds:0.01,
                music_seconds:2.0,beat:4.0,..Default::default() }).unwrap();
            assert!(output.errors.is_empty(),"{:?}",output.errors);
            scene.lua.load(r#"
for _,v in ipairs(PANES) do
    assert(v.timer:GetText()=='0:58')
    assert(v.histogram:GetNumVertices()>0 and v.life:GetNumVertices()>0)
end
"#).exec().unwrap();
            let panes=scene.lua.globals().get::<Table>("PANES").unwrap();
            for pane in panes.sequence_values::<Table>() {
                let params=scene.lua.create_table().unwrap();
                params.raw_set("Player",pane.unwrap().raw_get::<String>("player").unwrap()).unwrap();
                params.raw_set("TapNoteScore","TapNoteScore_W1").unwrap();
                crate::lua_util::broadcast_native_song_lua_message(&scene.lua,"Judgment",Some(Value::Table(params))).unwrap();
            }
            scene.lua.load(r#"
for _,v in ipairs(PANES) do
    assert(v.count:GetText()=='0001')
    MESSAGEMAN:Broadcast('HealthStateChanged',{PlayerNumber=v.player,HealthState='HealthState_Dead'})
end
"#).exec().unwrap();
            let output=scene.advance(SongLuaLiveFrame { screen_seconds:0.3,delta_seconds:0.29,
                music_seconds:3.0,beat:6.0,..Default::default() }).unwrap();
            assert!(output.errors.is_empty(),"{:?}",output.errors);
            scene.lua.load("for _,v in ipairs(PANES) do assert(v.timer:GetText()=='0:58') end").exec().unwrap();
        }
    }

    #[test]
    fn live_steps_density_uses_real_note_rows_and_chart_timing() {
        use deadsync_rules::timing::{TimingData, TimingSegments, FakeSegment, StopSegment, TimeSignatureSegment};
        let mut compiled = compile_with_context("steps-native-density", "return Def.ActorFrame {}", |context| {
            let timings = [TimingData::from_segments(0.0, 0.0, &TimingSegments {
                bpms: vec![(0.0, 120.0), (4.0, 240.0)],
                stops: vec![StopSegment { beat: 6.0, duration: 0.5 }],
                fakes: vec![FakeSegment { beat: 1.0, length: 1.0 }], ..Default::default()
            }, &[]), TimingData::from_segments(0.0, 0.0, &TimingSegments {
                bpms: vec![(0.0, 120.0)], time_signatures: vec![
                    TimeSignatureSegment { beat: 0.0, numerator: 3, denominator: 4 }], ..Default::default()
            }, &[])];
            context.player_step_indices = [Some(0), Some(1)];
            context.song_steps = (0..2).map(|index| crate::SongLuaStep {
                steps_type: "StepsType_Dance_Single".into(), difficulty: SongLuaDifficulty::Hard,
                description: format!("density{index}"), chart_name: String::new(), credit: String::new(),
                meter: 12, music_file: String::new(), possible_grade_points: 100 }).collect();
            context.song_step_timing = timings.to_vec();
            context.player_timing = [Some(timings[0].clone()), Some(timings[1].clone())];
            context.song_step_notes = vec![std::sync::Arc::from(&b"1100\n1000\n2000\n3000\nM000\n4000\n3000\nL000\nF000\n1000\n"[..]),
                std::sync::Arc::from(&b"1000\n0100\n1000\n0100\n1000\n0100\n"[..])];
            context.song_step_row_beats = vec![std::sync::Arc::from(&[0.0,1.0,2.0,3.0,4.0,5.0,6.0,7.0,8.0,9.0][..]),
                std::sync::Arc::from(&[0.0,2.0,3.0,5.0,6.0,8.0][..])];
        });
        let scene = compiled.live_scene.take().unwrap();
        scene.lua.load(r#"
local charts = GAMESTATE:GetCurrentSong():GetAllSteps()
local a, b = charts[1], charts[2]
assert(type(a.GetNotesPerMeasure) == 'function' and type(a.GetNpsPerMeasure) == 'function'
    and type(a.GetPeakNps) == 'function', 'Steps must expose real cached measure density')
local count, nps = a:GetNotesPerMeasure(), a:GetNpsPerMeasure()
assert(#count == 3 and count[1] == 2 and count[2] == 1 and count[3] == 1)
assert(nps[1] == 1 and math.abs(nps[2] - 2/3) < 0.000001 and nps[3] == 1)
assert(a:GetPeakNps() == 1 and a:GetPeakNps(PLAYER_2) == 1)
count[1], nps[1] = 999, 999
assert(a:GetNotesPerMeasure()[1] == 2 and a:GetNpsPerMeasure()[1] == 1)
local other = b:GetNotesPerMeasure()
assert(#other == 3 and other[1] == 2 and other[2] == 2 and other[3] == 2)
assert(b:GetPeakNps() == 1)
ParseChartInfo(a, 'P1')
ParseChartInfo(b, 'P2')
assert(SL.P1.Streams.PeakNPS == 1 and SL.P1.Streams.NotesPerMeasure[1] == 2)
assert(SL.P2.Streams.PeakNPS == 1 and SL.P2.Streams.NotesPerMeasure[2] == 2)
SL.P1.Streams.NotesPerMeasure[1] = 999
assert(a:GetNotesPerMeasure()[1] == 2)
GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(2)
assert(a:GetNpsPerMeasure()[1] == 1, 'cached density is independent of playback rate')
local ok = pcall(function() a:GetPeakNps('not a player') end)
assert(not ok)
"#).exec().unwrap();
    }

    #[test]
    fn live_underlay_header_uses_native_quad_texture_and_init() {
        for (width, height, enabled) in [(640.0, 480.0, [true, false]),
            (854.0, 480.0, [true, true]), (1280.0, 720.0, [false, true])] {
            let mut compiled = compile_with_context(&format!("underlay-header-{width}"),
                "return Def.ActorFrame {}", |context| {
                    context.screen_width = width;
                    context.screen_height = height;
                    context.players[0].enabled = enabled[0];
                    context.players[1].enabled = enabled[1];
                });
            let mut scene = compiled.live_scene.take().unwrap();
            let screen = scene.lua.globals().get::<Table>("__songlua_top_screen").unwrap();
            let underlay = crate::actor_children(&scene.lua, &screen).unwrap().raw_get::<Table>("Underlay").unwrap();
            let header = crate::actor_children(&scene.lua, &underlay).unwrap().raw_get::<Table>("Header").unwrap();
            assert!(crate::lua_util::actor_type_is(&header, "Quad").unwrap(),
                "Underlay Header must be the native Quad, including its default texture");
            scene.lua.globals().set("HEADER", header.clone()).unwrap();
            scene.lua.globals().set("UNDERLAY", underlay).unwrap();
            scene.lua.load(r#"
assert(HEADER:GetName() == 'Header' and HEADER:GetParent() == UNDERLAY)
assert(tostring(HEADER):find('Sprite', 1, true))
local texture = HEADER:GetTexture()
assert(texture ~= nil and texture:GetPath() == '')
assert(texture:GetSourceWidth() == 1 and texture:GetSourceHeight() == 1)
assert(texture:GetTextureWidth() == 1 and texture:GetTextureHeight() == 1)
assert(texture:GetImageWidth() == 1 and texture:GetImageHeight() == 1)
assert(HEADER:GetWidth() == 1 and HEADER:GetHeight() == 1)
assert(HEADER:GetZoomedWidth() == SCREEN_WIDTH and HEADER:GetZoomedHeight() == 80)
assert(HEADER:GetX() == SCREEN_WIDTH/2 and HEADER:GetY() == 0)
assert(HEADER:GetVAlign() == 0)
local color = HEADER:GetDiffuse()
assert(color[1] == 0 and color[2] == 0 and color[3] == 0 and math.abs(color[4] - 0.85) < 0.000001)
"#).exec().unwrap();
            let state = read_state(&scene.lua, &header).unwrap();
            assert_eq!((state.x, state.y), (width * 0.5, 0.0));
            assert_eq!((state.zoom_x, state.zoom_y), (width, 80.0));
            assert_eq!(state.diffuse, [0.0, 0.0, 0.0, 0.85]);
            let init = header.raw_get::<Function>("InitCommand").unwrap();
            assert_eq!(init.info().source.as_deref(), Some("@theme:/BGAnimations/ScreenGameplay underlay/Shared/Header.lua"));
            scene.lua.load("HEADER:xy(1,2):diffuse(1,1,1,1):zoomto(4,5):valign(1)").exec().unwrap();
            init.call::<()>(header.clone()).unwrap();
            let output = scene.advance(SongLuaLiveFrame { delta_seconds: 0.01, screen_seconds: 0.01,
                ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let restored = read_state(&scene.lua, &header).unwrap();
            assert_eq!((restored.x, restored.y, restored.zoom_x, restored.zoom_y),
                (width * 0.5, 0.0, width, 80.0));
            assert_eq!(restored.diffuse, state.diffuse);
        }
    }

    #[test]
    fn live_shared_bpm_owns_native_hierarchy_and_updates_real_timing_and_rate() {
        use deadsync_rules::timing::{TimingData, TimingSegments};
        for (enabled, split, upper) in [([true,false],false,false), ([true,true],false,false),
            ([true,true],true,false), ([false,true],true,true), ([true,false],false,true)] {
            let mut compiled=compile_with_context(&format!("shared-bpm-{enabled:?}-{split}-{upper}"),
                "return Def.ActorFrame {}", |context| {
                    context.defer_initial_live_update=true;
                    context.song_display_bpms=[666.0,999.0];
                    context.player_step_indices=if split {[Some(0),Some(1)]}else{[Some(0),Some(0)]};
                    for player in 0..LUA_PLAYERS {
                        context.players[player].enabled=enabled[player];
                        context.players[player].nps_graph_at_top=upper;
                        context.players[player].screen_x=320.0;
                        context.players[player].difficulty=SongLuaDifficulty::Hard;
                    }
                    let timings=[TimingData::from_segments(0.0,0.0,&TimingSegments {
                        bpms:vec![(0.0,120.0),(2.0,150.0)],..Default::default()},&[]),
                        TimingData::from_segments(0.0,0.0,&TimingSegments {
                        bpms:vec![(0.0,240.0),(2.0,300.0)],..Default::default()},&[])];
                    context.player_timing=[Some(timings[0].clone()),Some(timings[usize::from(split)].clone())];
                    context.song_step_timing=timings.to_vec();
                    context.song_steps=(0..2).map(|index|crate::SongLuaStep {
                        steps_type:"StepsType_Dance_Single".into(),difficulty:SongLuaDifficulty::Hard,
                        description:format!("chart{index}"),chart_name:String::new(),credit:String::new(),
                        meter:12,music_file:String::new(),possible_grade_points:100}).collect();
                });
            let mut scene=compiled.live_scene.take().unwrap();
            let screen=scene.lua.globals().get::<Table>("__songlua_top_screen").unwrap();
            let underlay=crate::actor_children(&scene.lua,&screen).unwrap().raw_get::<Table>("Underlay").unwrap();
            let outer=crate::lua_util::actor_direct_children(&scene.lua,&underlay).unwrap().into_iter()
                .find(|owner|crate::actor_children(&scene.lua,owner).unwrap()
                    .raw_get::<Option<Table>>("RatemodDisplay").unwrap().is_some());
            let outer=outer.expect("Underlay must own the native shared BPM frame hierarchy");
            let outer_parts=crate::lua_util::actor_direct_children(&scene.lua,&outer).unwrap();
            assert_eq!(outer_parts.len(),2);
            let rate=outer_parts[0].clone();let inner=outer_parts[1].clone();
            assert_eq!(crate::lua_util::actor_name(&rate).unwrap(),"RatemodDisplay");
            assert_eq!(crate::lua_util::actor_name(&inner).unwrap(),"");
            let texts=crate::lua_util::actor_direct_children(&scene.lua,&inner).unwrap();
            let dual=enabled==[true,true]&&split;
            assert_eq!(texts.len(),if dual{2}else{1});
            assert_eq!(crate::lua_util::actor_name(&texts[0]).unwrap(),if dual{"DisplayP1"}else{"BPMDisplay"});
            let pose=read_state(&scene.lua,&outer).unwrap();
            let master=usize::from(!enabled[0]);
            assert_eq!(pose.x,320.0+if upper {if master==0 {192.0}else{-192.0}}else{0.0});
            assert_eq!(pose.y,52.0);assert!((pose.zoom-1.33).abs()<1e-6);
            assert_eq!(read_state(&scene.lua,&rate).unwrap().y,12.0);
            assert!((read_state(&scene.lua,&rate).unwrap().zoom-0.5).abs()<1e-6);
            for text in &texts {
                assert_eq!(text.raw_get::<String>("Font").unwrap(),"Common Normal");
                assert_eq!(text.get::<Function>("GetText").unwrap().call::<String>(text.clone()).unwrap(),"");
                assert_eq!(text.get::<Function>("GetParent").unwrap().call::<Table>(text.clone()).unwrap(),inner);
            }
            scene.lua.globals().set("BPM_UNDERLAY",underlay).unwrap();
            scene.lua.globals().set("BPM_OUTER",outer.clone()).unwrap();
            scene.lua.globals().set("BPM_INNER",inner).unwrap();
            scene.lua.load("assert(BPM_UNDERLAY:GetChild('BPMDisplay')==nil); assert(BPM_OUTER:GetChild('BPMDisplay')==nil); assert(BPM_OUTER:GetParent()==BPM_UNDERLAY); assert(BPM_INNER:GetParent()==BPM_OUTER)").exec().unwrap();
            for (seconds,music_rate) in [(0.25,1.0),(1.5,1.5)] {
                let game=scene.lua.globals().get::<Table>("GAMESTATE").unwrap();
                let options=game.get::<Function>("GetSongOptionsObject").unwrap()
                    .call::<Table>((game,"ModsLevel_Song")).unwrap();
                options.get::<Function>("MusicRate").unwrap().call::<mlua::MultiValue>((options,music_rate)).unwrap();
                let output=scene.advance(SongLuaLiveFrame { screen_seconds:seconds,music_seconds:seconds,
                    delta_seconds:0.25,..Default::default() }).unwrap();
                assert!(output.errors.is_empty(),"{:?}",output.errors);
                for (index,text) in texts.iter().enumerate() {
                    let player=if dual{index}else{master};let fast=player==1&&split;
                    let bpm=if seconds==0.25 {if fast{240.0}else{120.0}}else if fast{300.0}else{150.0};
                    let expected=format!("{:.0}",bpm*music_rate);
                    assert_eq!(text.get::<Function>("GetText").unwrap().call::<String>(text.clone()).unwrap(),expected);
                }
                assert_eq!(rate.get::<Function>("GetText").unwrap().call::<String>(rate.clone()).unwrap(),
                    if music_rate==1.0{""}else{"1.50x rate"});
            }
        }
    }

    #[test]
    fn live_standard_life_children_follow_native_subactor_table_construction() {
        let compiled = compile("standard-life-child-table", "return Def.ActorFrame {}");
        let scene = compiled.live_scene.expect("real standard life owners");
        let screen = scene.lua.globals().get::<Table>("__songlua_top_screen").unwrap();
        let underlay = crate::actor_children(&scene.lua, &screen).unwrap().raw_get::<Table>("Underlay").unwrap();
        let meters = crate::lua_util::actor_direct_children(&scene.lua, &underlay).unwrap()
            .into_iter().filter(|actor| crate::actor_children(&scene.lua, actor).unwrap()
                .raw_get::<Option<Table>>("MeterSwoosh").unwrap().is_some()).collect::<Vec<_>>();
        assert_eq!(meters.len(), 2, "inspect both enabled native Standard owners");
        for meter in meters {
            let parts = crate::lua_util::actor_direct_children(&scene.lua, &meter).unwrap();
            assert_eq!(parts.len(), 4);
            let ordered = scene.lua.create_sequence_from(parts).unwrap();
            scene.lua.globals().set("STANDARD_OWNER", meter).unwrap();
            scene.lua.globals().set("STANDARD_SUBACTORS", ordered).unwrap();
            scene.lua.load(r#"
local owner, parts = STANDARD_OWNER, STANDARD_SUBACTORS
assert(parts[1]:GetName() == '' and parts[2]:GetName() == '')
assert(parts[3]:GetName() == 'MeterFill' and parts[4]:GetName() == 'MeterSwoosh')
-- ActorFrame.cpp::PushChildrenTable starts empty and inserts each m_SubActor.
-- Standard.lua supplies two empty names, then MeterFill, then MeterSwoosh.
local expected = {}
rawset(expected, '', parts[1])
rawset(expected, '', {parts[1], parts[2]})
rawset(expected, 'MeterFill', parts[3])
rawset(expected, 'MeterSwoosh', parts[4])
local actual, wanted, observed = owner:GetChildren(), {}, {}
for name in pairs(expected) do wanted[#wanted + 1] = name end
for name in pairs(actual) do observed[#observed + 1] = name end
assert(table.concat(observed, '|') == table.concat(wanted, '|'),
    'Standard GetChildren must follow native subactor table construction')
assert(actual.MeterFill == parts[3] and actual.MeterSwoosh == parts[4])
assert(actual[''][1] == parts[1] and actual[''][2] == parts[2])
for _, part in ipairs(parts) do assert(part:GetParent() == owner) end
actual.MeterFill = nil
local fresh = owner:GetChildren()
assert(fresh ~= actual and fresh.MeterFill == parts[3])
assert(owner:GetChild('MeterFill') == parts[3] and owner:GetChild('MeterSwoosh') == parts[4])
"#).exec().expect("native child insertion history, identities, groups and snapshot ownership");
        }
    }

    #[test]
    fn live_underlay_standard_life_meter_uses_real_children_and_life_events() {
        use deadsync_rules::timing::{DelaySegment, StopSegment, TimingData, TimingSegments};
        for (width, enabled, hidden, expected) in [
            (640.0, [true, false], [false, false], 1),
            (854.0, [true, true], [false, false], 2),
            (854.0, [false, true], [false, false], 1),
            (640.0, [true, false], [true, false], 0),
        ] {
            let source = r#"
LIFE_METERS = {}
return Def.Actor { OnCommand = function()
    local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
    local owners = underlay:GetChildren()['']
    if owners and tostring(owners):find('ActorFrame', 1, true) then owners = {owners} end
    for _, owner in ipairs(owners or {}) do
        if owner.GetChild and owner:GetChild('MeterFill') and owner:GetChild('MeterSwoosh') then
            assert(owner:GetParent() == underlay and owner:GetName() == '')
            assert(owner:GetNumChildren() == 4)
            assert(owner:GetVisible() and owner:GetY() == 20)
            LIFE_METERS[#LIFE_METERS + 1] = owner
        end
    end
    assert(#LIFE_METERS == $COUNT, 'Underlay must load the selected native Standard LifeMeter')
end }
"#.replace("$COUNT", &expected.to_string());
            let mut compiled = compile_with_context(&format!("standard-life-{width}-{enabled:?}-{hidden:?}"), &source, |context| {
                context.screen_width = width;
                for player in 0..LUA_PLAYERS {
                    context.players[player].enabled = enabled[player];
                    context.players[player].hide_lifebar = hidden[player];
                }
                context.player_timing = [
                    Some(TimingData::from_segments(0.0, 0.0, &TimingSegments {
                        bpms: vec![(0.0, 120.0)], stops: vec![StopSegment { beat: 1.0, duration: 0.5 }],
                        ..Default::default() }, &[])),
                    Some(TimingData::from_segments(0.0, 0.0, &TimingSegments {
                        bpms: vec![(0.0, 240.0)], delays: vec![DelaySegment { beat: 1.0, duration: 0.5 }],
                        ..Default::default() }, &[])),
                ];
            });
            let mut scene = compiled.live_scene.take().expect("real Standard life meters");
            let meters = scene.lua.globals().get::<Table>("LIFE_METERS").unwrap()
                .sequence_values::<Table>().map(Result::unwrap).collect::<Vec<_>>();
            let visible = (0..LUA_PLAYERS).filter(|p| enabled[*p] && !hidden[*p]).collect::<Vec<_>>();
            assert_eq!(meters.len(), expected);
            for (meter, player) in meters.iter().zip(&visible) {
                let parts = crate::lua_util::actor_direct_children(&scene.lua, meter).unwrap();
                assert_eq!(parts.len(), 4);
                let x = width * 0.5 + if *player == 0 { -1.0 } else { 1.0 }
                    * if width == 640.0 { 238.0 } else { 288.0 };
                for (index, width) in [(0, 140.0), (1, 136.0)] {
                    assert_eq!(parts[index].get::<Function>("GetX").unwrap().call::<f32>(parts[index].clone()).unwrap(), x);
                    assert_eq!(parts[index].get::<Function>("GetZoomedWidth").unwrap().call::<f32>(parts[index].clone()).unwrap(), width);
                }
                assert_eq!(read_state(&scene.lua, &parts[1]).unwrap().diffuse, [0.0, 0.0, 0.0, 1.0]);
                assert_eq!(read_state(&scene.lua, &parts[2]).unwrap().zoom_x, 0.0,
                    "native initial LifeMeter::Load message starts from zero fill width");
                assert!((crate::actor_tween_time_left(&scene.lua, meter).unwrap() - 0.1).abs() < 1e-6);
                for part in &parts[2..] {
                    assert_eq!(part.get::<Function>("GetX").unwrap().call::<f32>(part.clone()).unwrap(), x - 68.0);
                    assert!((part.get::<Function>("GetZoomedWidth").unwrap().call::<f32>(part.clone()).unwrap() - 68.0).abs() < 1e-5);
                    assert!(part.raw_get::<Option<Function>>("LifeChangedMessageCommand").unwrap().is_some());
                    assert!(part.raw_get::<Option<Function>>("HealthStateChangedMessageCommand").unwrap().is_some());
                }
                let texture = parts[3].get::<Function>("GetTexture").unwrap().call::<Table>(parts[3].clone()).unwrap();
                assert_eq!(texture.get::<Function>("GetSourceWidth").unwrap().call::<u32>(texture.clone()).unwrap(), 128);
                assert_eq!(texture.get::<Function>("GetSourceHeight").unwrap().call::<u32>(texture).unwrap(), 16);
                let path = crate::actor_texture_path(&parts[3]).unwrap().unwrap();
                let bitmap = deadlib_assets::generated_texture(&path.to_string_lossy()).unwrap();
                assert!(bitmap.image.pixels().any(|pixel| pixel[3] != 0));
                assert!((read_state(&scene.lua, &parts[3]).unwrap().diffuse[3] - 0.2).abs() < 1e-6);
            }
            for (seconds, delta) in [(0.1, 0.1), (0.6, 0.5), (1.4, 0.8)] {
                let output = scene.advance(SongLuaLiveFrame { screen_seconds: seconds, music_seconds: seconds,
                    delta_seconds: delta, ..Default::default() }).unwrap();
                assert!(output.errors.is_empty(), "{:?}", output.errors);
                for (meter, player) in meters.iter().zip(&visible) {
                    let parts = crate::lua_util::actor_direct_children(&scene.lua, meter).unwrap();
                    assert!((read_state(&scene.lua, &parts[2]).unwrap().zoom_x - 68.0).abs() < 1e-5);
                    let pose = read_state(&scene.lua, &parts[3]).unwrap();
                    assert!((pose.zoom * pose.zoom_x * 128.0 - 68.0).abs() < 1e-5);
                    assert_eq!(pose.texcoord_velocity.unwrap()[0], if seconds == 0.6 { 0.0 } else { -(*player as f32 + 1.0) });
                }
            }
            if enabled[0] && !hidden[0] {
                scene.lua.load("SCREENMAN:GetTopScreen():GetChild('PlayerP1'):SetLife(1)").exec().unwrap();
                let fill = crate::lua_util::actor_direct_children(&scene.lua, &meters[0]).unwrap()[2].clone();
                assert!((fill.get::<Function>("GetZoomedWidth").unwrap().call::<f32>(fill.clone()).unwrap() - 136.0).abs() < 1e-5);
                let mut frame = SongLuaLiveFrame { screen_seconds: 1.5, music_seconds: 1.5,
                    delta_seconds: 0.1, ..Default::default() };
                frame.players[0].life = 1.0;
                let output = scene.advance(frame).unwrap();
                assert!(output.errors.is_empty(), "{:?}", output.errors);
                let parts = crate::lua_util::actor_direct_children(&scene.lua, &meters[0]).unwrap();
                assert_eq!(read_state(&scene.lua, &parts[2]).unwrap().diffuse, [1.0; 4]);
                assert_eq!(read_state(&scene.lua, &parts[3]).unwrap().diffuse[3], 1.0);
                if meters.len() == 2 {
                    let other = crate::lua_util::actor_direct_children(&scene.lua, &meters[1]).unwrap();
                    assert!((read_state(&scene.lua, &other[2]).unwrap().zoom_x - 68.0).abs() < 1e-5);
                    assert!((read_state(&scene.lua, &other[3]).unwrap().diffuse[3] - 0.2).abs() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn live_underlay_difficulty_meter_owns_native_quad_and_text() {
        for (width, enabled_p1, enabled_p2) in [(640.0, true, false), (854.0, true, true), (854.0, false, true)] {
            let source = r#"
DIFFICULTY_METERS = {}
return Def.Actor { OnCommand = function()
    local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
    local unnamed = underlay:GetChildren()['']
    if unnamed and tostring(unnamed):find('ActorFrame', 1, true) then unnamed = {unnamed} end
    for _, owner in ipairs(unnamed or {}) do
        if owner.GetNumChildren and owner:GetNumChildren() == 2 then
            local parts = owner:GetChildren()['']
            if parts and #parts == 2 and tostring(parts[1]):find('Sprite', 1, true)
                and tostring(parts[2]):find('BitmapText', 1, true) then
                assert(owner:GetParent() == underlay and owner:GetName() == '')
                assert(parts[1]:GetParent() == owner and parts[2]:GetParent() == owner)
                assert(parts[1]:GetName() == '' and parts[2]:GetName() == '')
                assert(parts[1]:GetZoomedWidth() == 30 and parts[1]:GetZoomedHeight() == 30)
                assert(parts[1]:GetTexture():GetPath() == '')
                assert(math.abs(parts[2]:GetZoom() - 0.4) < 0.000001)
                DIFFICULTY_METERS[#DIFFICULTY_METERS + 1] = owner
            end
        end
    end
    assert(#DIFFICULTY_METERS == #GAMESTATE:GetHumanPlayers(),
        'Underlay must load each human player native DifficultyMeter frame')
end }
"#;
            let mut compiled = compile_with_context(&format!("underlay-difficulty-{width}-{enabled_p1}-{enabled_p2}"), source, |context| {
                context.screen_width = width;
                context.players[0].enabled = enabled_p1;
                context.players[1].enabled = enabled_p2;
                context.players[0].difficulty = SongLuaDifficulty::Hard;
                context.players[1].difficulty = SongLuaDifficulty::Easy;
                context.player_step_indices = [Some(0), Some(1)];
                context.song_steps = vec![
                    crate::SongLuaStep { steps_type: "StepsType_Dance_Single".into(), difficulty: SongLuaDifficulty::Hard,
                        description: String::new(), chart_name: String::new(), credit: String::new(), meter: 12,
                        music_file: String::new(), possible_grade_points: 100 },
                    crate::SongLuaStep { steps_type: "StepsType_Dance_Single".into(), difficulty: SongLuaDifficulty::Easy,
                        description: String::new(), chart_name: String::new(), credit: String::new(), meter: 4,
                        music_file: String::new(), possible_grade_points: 100 },
                ];
            });
            let mut scene = compiled.live_scene.take().expect("actual DifficultyMeter actors");
            let owners = scene.lua.globals().get::<Table>("DIFFICULTY_METERS").unwrap()
                .sequence_values::<Table>().map(Result::unwrap).collect::<Vec<_>>();
            let players = [enabled_p1, enabled_p2].iter().enumerate()
                .filter_map(|(player, enabled)| enabled.then_some(player)).collect::<Vec<_>>();
            for (owner, player) in owners.iter().zip(players) {
                let state = read_state(&scene.lua, owner).unwrap();
                let sign = if player == 0 { -1.0 } else { 1.0 };
                let offset = if width == 640.0 { 292.5 } else { 342.5 };
                assert_eq!(state.x, width * 0.5 + sign * offset);
                assert_eq!(state.y, 56.0);
                let parts = crate::lua_util::actor_direct_children(&scene.lua, owner).unwrap();
                let text = &parts[1];
                assert_eq!(text.raw_get::<String>("Font").unwrap(), "Common Bold");
                assert_eq!(text.get::<Function>("GetText").unwrap().call::<String>(text.clone()).unwrap(), if player == 0 { "12" } else { "4" });
                assert_eq!(read_state(&scene.lua, text).unwrap().diffuse, [0.0, 0.0, 0.0, 1.0]);
                let color = scene.lua.globals().get::<Function>("DifficultyColor").unwrap()
                    .call::<Table>(if player == 0 { "Difficulty_Hard" } else { "Difficulty_Easy" }).unwrap();
                assert_eq!(read_state(&scene.lua, &parts[0]).unwrap().diffuse,
                    std::array::from_fn(|index| color.raw_get::<f32>(index + 1).unwrap()));
                for part in &parts {
                    assert!(part.raw_get::<Option<Function>>("BeginCommand").unwrap().is_some());
                    assert!(part.raw_get::<Option<Function>>("CurrentSongChangedMessageCommand").unwrap().is_some());
                }
            }
            scene.lua.load(r#"
local steps = GAMESTATE:GetCurrentSong():GetAllSteps()
for _, player in ipairs(GAMESTATE:GetHumanPlayers()) do GAMESTATE:SetCurrentSteps(player, steps[2]) end
MESSAGEMAN:Broadcast('CurrentSongChanged')
"#).exec().unwrap();
            let output = scene.advance(SongLuaLiveFrame { delta_seconds: 0.01, screen_seconds: 0.01,
                ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            for owner in owners {
                let parts = crate::lua_util::actor_direct_children(&scene.lua, &owner).unwrap();
                assert_eq!(parts[1].get::<Function>("GetText").unwrap().call::<String>(parts[1].clone()).unwrap(), "4");
            }
        }
    }

    #[test]
    fn live_underlay_background_filter_uses_initial_profile_inputs() {
        for (style, p1, p2, enabled, expected_width, expected_count) in [
            ("single", 75, 0, false, 256, 1),
            ("single", 50, 95, true, 256, 2),
            ("double", 37, 0, false, 512, 1),
            ("routine", 75, 95, true, 512, 1),
            ("single", 0, 0, false, 256, 0),
        ] {
            let source = r#"
FILTERS = {}
return Def.Actor { OnCommand = function()
    local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
    local unnamed = underlay:GetChildren()['']
    if unnamed and (tostring(unnamed):find('Sprite', 1, true) or tostring(unnamed):find('ActorFrame', 1, true)) then unnamed = {unnamed} end
    for _, child in ipairs(unnamed or {}) do
        if tostring(child):find('Sprite', 1, true) then
            assert(child:GetParent() == underlay and child:GetName() == '')
            assert(child:GetZoomedWidth() == $WIDTH and child:GetZoomedHeight() == SCREEN_HEIGHT)
            assert(child:GetTexture() ~= nil)
            FILTERS[#FILTERS+1] = child
        end
    end
    assert(#FILTERS == $COUNT, 'Underlay must load the profile-selected BackgroundFilter Quad')
end }
"#.replace("$WIDTH", &expected_width.to_string()).replace("$COUNT", &expected_count.to_string());
            let compiled = compile_with_context(&format!("underlay-filter-{style}-{p1}-{p2}"), &source, |context| {
                context.style_name = style.into();
                context.players[0].background_filter_percent = p1;
                context.players[1].background_filter_percent = p2;
                context.players[1].enabled = enabled;
                context.players[0].screen_x = 130.0;
                context.players[1].screen_x = 510.0;
            });
            let scene = compiled.live_scene.expect("selected native background filters");
            let filters = scene.lua.globals().get::<Table>("FILTERS").unwrap();
            assert_eq!(filters.raw_len(), expected_count);
            for (index, filter) in filters.sequence_values::<Table>().map(Result::unwrap).enumerate() {
                let state = read_state(&scene.lua, &filter).unwrap();
                assert_eq!(state.x, [130.0, 510.0][index]);
                assert_eq!(state.y, scene.context.screen_height * 0.5);
                let percent = [p1, p2][index];
                assert!((state.diffuse[3] - f32::from(percent) / 100.0).abs() < 1e-6);
                assert_eq!(&state.diffuse[..3], &[0.0; 3]);
                assert!(filter.raw_get::<Option<Function>>("OffCommand").unwrap().is_some());
                assert!(filter.raw_get::<Option<Function>>("ComboFlashCommand").unwrap().is_some());
            }
        }
    }

    #[test]
    fn live_in_transition_loads_native_style_bitmaps_and_poses() {
        let compiled = compile("in-transition-bitmaps", r#"
IN_SPRITES = {}
return Def.Actor { OnCommand = function()
    local transition = SCREENMAN:GetTopScreen():GetChild('In')
    local loaded = transition:GetChildren()['']
    local visuals = loaded:GetChildren()[''][1]
    local children = visuals:GetChildren()['']
    assert(#children == 4)
    for index = 2, 4 do
        local sprite = children[index]
        local texture = assert(sprite:GetTexture(), 'In visual Sprite must load its native bitmap')
        assert(sprite:GetParent() == visuals and sprite:GetName() == '')
        assert(sprite:GetWidth() == 640 and sprite:GetHeight() == 480)
        assert(texture:GetNumFrames() == 1)
        assert(texture:GetPath():find('/Technique/GameplayIn ', 1, true))
        assert(sprite:GetX() == SCREEN_CENTER_X and sprite:GetY() == SCREEN_CENTER_Y)
        local destination = ({1.1, 1.3, 0.9})[index - 1]
        assert(math.abs(sprite:GetZoomX() - destination) < 0.000001)
        assert(math.abs(sprite:GetZoomY() - destination) < 0.000001)
        IN_SPRITES[#IN_SPRITES + 1] = sprite
    end
    assert(ThemePrefs.Get('VisualStyle') == 'Technique')
end }
"#);
        let scene = compiled.live_scene.expect("native In resource construction");
        let sprites = scene.lua.globals().get::<Table>("IN_SPRITES").unwrap();
        assert_eq!(sprites.raw_len(), 3, "inspect all three native In sprites");
        let sprites = sprites.sequence_values::<Table>().map(Result::unwrap).collect::<Vec<_>>();
        let paths = sprites.iter().map(|sprite| crate::actor_texture_path(sprite).unwrap().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(paths[0], paths[1], "the two splodes share the same native resource");
        assert_ne!(paths[0], paths[2], "minisplode retains its own bitmap");
        for path in &paths {
            let texture = deadlib_assets::generated_texture(&path.to_string_lossy()).unwrap();
            assert_eq!(texture.source_size, [1280, 960]);
            assert!(texture.image.pixels().any(|pixel| pixel[3] != 0), "native bitmap contains visible pixels");
        }
        for (index, sprite) in sprites.iter().enumerate() {
            assert_eq!(crate::actor_tween_time_left(&scene.lua, sprite).unwrap(), if index == 2 { 1.2 } else { 1.0 });
            let pose = read_state(&scene.lua, sprite).unwrap();
            assert_eq!(pose.zoom * pose.zoom_x, 0.0, "native rendered X scale starts at zero");
            assert_eq!(pose.zoom * pose.zoom_y, 0.0, "native rendered Y scale starts at zero");
            assert_eq!(read_state(&scene.lua, sprite).unwrap().rot_z_deg, if index == 1 { -10.0 } else { 10.0 });
        }
    }

    #[test]
    fn live_out_transition_loads_quad_directly_with_native_commands() {
        let mut compiled = compile("out-transition-direct-quad", r#"
return Def.Actor { OnCommand = function()
    OUT = SCREENMAN:GetTopScreen():GetChild('Out')
    assert(OUT:GetNumChildren() == 1)
    OUT_QUAD = OUT:GetChildren()['']
    assert(tostring(OUT_QUAD):find('Sprite', 1, true),
        'Out must load its native Quad directly without an extra ActorFrame')
    assert(OUT_QUAD:GetParent() == OUT and OUT_QUAD:GetName() == '')
    assert(OUT_QUAD.GetNumChildren == nil)
    assert(OUT_QUAD:GetZoomedWidth() == SCREEN_WIDTH)
    assert(OUT_QUAD:GetZoomedHeight() == SCREEN_HEIGHT)
    assert(type(OUT_QUAD.OffCommand) == 'function')
end }
"#);
        let mut scene = compiled.live_scene.take().expect("native Out loaded child");
        let out = scene.lua.globals().get::<Table>("OUT").unwrap();
        let quad = scene.lua.globals().get::<Table>("OUT_QUAD").unwrap();
        let children = crate::lua_util::actor_direct_children(&scene.lua, &out).unwrap();
        assert_eq!(children.len(), 1, "inspect the actual transition owner");
        assert_eq!(children[0].to_pointer(), quad.to_pointer());
        assert_eq!(out.raw_get::<Table>("__songlua_transition_child").unwrap(), quad);
        assert_eq!(read_state(&scene.lua, &quad).unwrap().diffuse, [0.0; 4]);
        assert_eq!(crate::actor_tween_time_left(&scene.lua, &quad).unwrap(), 1.5);
        scene.lua.load("OUT_QUAD:playcommand('Off')").exec()
            .expect("native Off handler with fresh human profiles");
        scene.advance(SongLuaLiveFrame { screen_seconds: 0.25, delta_seconds: 0.25,
            ..Default::default() }).unwrap();
        assert_eq!(crate::actor_tween_time_left(&scene.lua, &quad).unwrap(), 1.5,
            "waiting transition freezes its loaded Quad");
        assert_eq!(read_state(&scene.lua, &quad).unwrap().diffuse[3], 0.0);
        crate::lua_util::start_actor_transition(&scene.lua, &out).unwrap();
        for (seconds, delta, left, alpha) in [
            (0.5, 0.25, 1.25, 0.0), (0.75, 0.25, 1.0, 0.0),
            (1.25, 0.5, 0.5, 0.5), (1.75, 0.5, 0.0, 1.0),
        ] {
            let output = scene.advance(SongLuaLiveFrame { screen_seconds: seconds,
                delta_seconds: delta, ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            assert!((crate::actor_tween_time_left(&scene.lua, &out).unwrap() - left).abs() < 1e-6);
            assert!((read_state(&scene.lua, &quad).unwrap().diffuse[3] - alpha).abs() < 1e-6);
        }
        scene.advance(SongLuaLiveFrame { screen_seconds: 1.76, delta_seconds: 0.01,
            ..Default::default() }).unwrap();
        assert_eq!(out.raw_get::<u8>("__songlua_transition_state").unwrap(), 2);
    }

    #[test]
    fn live_transition_getter_uses_loaded_child_and_native_lifecycle() {
        let mut compiled = compile("transition-virtual-getter", r#"
OUT = SCREENMAN:GetTopScreen():GetChild('Out')
return Def.Actor { OnCommand=function(self)
    OUT:smooth(7):diffusealpha(0)
    assert(OUT:GetTweenTimeLeft() == 0, 'waiting transition ignores its own queue')
end }
"#);
        let mut scene = compiled.live_scene.take().expect("transition scene");
        let out = scene.lua.globals().get::<Table>("OUT").unwrap();
        let child = out.raw_get::<Table>("__songlua_transition_child").unwrap();
        assert_eq!(crate::actor_tween_time_left(&scene.lua, &child).unwrap(), 1.5);
        scene.advance(SongLuaLiveFrame { screen_seconds: 0.25, delta_seconds: 0.25,
            ..Default::default() }).unwrap();
        assert_eq!(crate::actor_tween_time_left(&scene.lua, &child).unwrap(), 1.5,
            "waiting transition freezes its loaded child");
        crate::lua_util::start_actor_transition(&scene.lua, &out).unwrap();
        assert_eq!(crate::actor_tween_time_left(&scene.lua, &out).unwrap(), 1.5);
        for (seconds, delta, expected) in [(0.5, 0.25, 1.25), (1.75, 1.25, 0.0), (2.0, 0.25, 0.0)] {
            let output = scene.advance(SongLuaLiveFrame { screen_seconds: seconds,
                delta_seconds: delta, ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            assert_eq!(crate::actor_tween_time_left(&scene.lua, &out).unwrap(), expected);
        }
        assert_eq!(out.raw_get::<u8>("__songlua_transition_state").unwrap(), 2,
            "completion is checked before the next child update");
        crate::lua_util::start_actor_transition(&scene.lua, &out).unwrap();
        assert_eq!(out.raw_get::<u8>("__songlua_transition_state").unwrap(), 2,
            "a finished transition cannot restart before Reset");
    }

    #[test]
    fn live_outro_rebuilds_song_options_after_queued_screen_message() {
        let mut compiled = compile("outro-options", r#"
local ps = GAMESTATE:GetPlayerState(PLAYER_1)
PREFERRED = ps:GetPlayerOptions('ModsLevel_Preferred')
STAGE = ps:GetPlayerOptions('ModsLevel_Stage')
SONG = ps:GetPlayerOptions('ModsLevel_Song')
CURRENT = ps:GetPlayerOptions('ModsLevel_Current')
SPEEDS = {}
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta <= 0 then return end
        local value, speed = SONG:Reverse(0, 1000)
        SPEEDS[#SPEEDS + 1] = {value, speed}
    end)
end }
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        scene.lua.load(r#"
            PREFERRED:Reverse(0.2, 0.25)
            STAGE:Reverse(0.4, 0.5)
            SONG:Reverse(1, 1000)
            CURRENT:Reverse(0)
        "#).exec().expect("distinct option levels");
        let mut frame = SongLuaLiveFrame { screen_seconds: 0.125, delta_seconds: 0.125,
            ..Default::default() };
        for index in 0..4 {
            frame.gameplay_outro = index > 0;
            scene.advance(frame.clone()).expect("outro frame");
            if index == 2 {
                let current = scene.lua.load("return CURRENT:Reverse()")
                    .eval::<f32>().expect("current retained after Song assignment");
                assert_eq!(current, 0.0);
                assert_eq!(scene.lua.load("return SONG:Reverse()")
                    .eval::<f32>().expect("stage copied after actor callbacks"), 0.4);
            }
            frame.screen_seconds += 0.125;
        }
        let speeds = scene.lua.globals().get::<Table>("SPEEDS").expect("actual setter returns");
        for (index, value, speed) in [(1, 1.0, 1000.0), (2, 0.0, 1000.0),
                                     (3, 0.0, 1000.0), (4, 0.4, 0.5)] {
            let result = speeds.raw_get::<Table>(index).expect("callback result");
            assert_eq!(result.raw_get::<f32>(1).expect("previous value"), value);
            assert_eq!(result.raw_get::<f32>(2).expect("previous approach speed"), speed);
        }
        assert_eq!(scene.lua.load("return CURRENT:Reverse()")
            .eval::<f32>().expect("Current approaches the rebuilt Song target"), 0.0625);
        scene.advance(frame.clone()).expect("continued outro");
        assert_eq!(speeds.raw_get::<Table>(5).expect("continued setter").raw_get::<f32>(2)
            .expect("reset occurs once"), 1000.0);
        assert_eq!(scene.lua.load("return PREFERRED:Reverse(), STAGE:Reverse(), SONG == GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')")
            .eval::<(f32, f32, bool)>().expect("retained references"), (0.2, 0.4, true));
    }

    #[test]
    fn actor_parent_loading() {
        let mut compiled = compile("parent-loading", r#"
return Def.ActorFrame{Name='DeclaredRoot',
 Def.ActorFrame{InitCommand=function(self)
  local parent=self:GetParent()
  assert(parent:GetName()=='')
  parent:name('from-child')
  assert(parent:GetName()=='from-child')
 end},
 InitCommand=function(self) assert(self:GetName()=='DeclaredRoot') end,
 OnCommand=function(self)
  local parent=self:GetParent()
  self:SetUpdateFunction(function(self)
   assert(self:GetParent()==parent)
   assert(parent:GetName()=='SongForeground')
   assert(parent:GetParent()==SCREENMAN:GetTopScreen())
   assert(parent:GetParent():GetParent()==nil)
   self:aux(1)
  end)
 end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        for seconds in [0.0, 0.125] {
            let output = scene.advance(SongLuaLiveFrame {
                music_seconds: seconds, screen_seconds: seconds,
                delta_seconds: 0.125, ..Default::default()
            }).expect("owned update");
            assert!(output.errors.is_empty(), "{:?}", output.errors);
        }
        crate::lua_util::run_actor_update_functions_for_table(
            &scene.lua, scene.root.as_table().expect("session roots"), 0.0,
        ).expect("direct update");
        let output = take_output(&scene.lua).expect("direct output");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let actor: Table = scene.root.as_table().expect("session roots").raw_get(1).expect("root");
        assert_eq!(actor.get::<Function>("getaux").expect("aux getter")
            .call::<f32>(actor.clone()).expect("updated aux"), 1.0);
    }

    #[test]
    fn explicit_actor_children() {
        let mut compiled = compile("explicit-children", r#"
return Def.ActorFrame{
 Name='Root',
 Def.Quad{InitCommand=function() error('excluded direct child') end},
 children={
  [1]=Def.Quad{Name='Head',InitCommand=function(self) self:x(1) end},
  [3]=Def.ActorFrame{Name='Tail',InitCommand=function(self) self:x(3) end},
  named=Def.Quad{Name='Named',InitCommand=function(self) self:x(7) end},
 },
 OnCommand=function(self)
  assert(self:GetNumChildren()==3)
  assert(self:GetChild('Head'):GetX()==1)
  assert(self:GetChild('Tail'):GetX()==3)
  assert(self:GetChild('Named'):GetX()==7)
  self:aux(1)
 end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        let output = scene.advance(SongLuaLiveFrame::default()).expect("child activation");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let actor: Table = scene.root.as_table().expect("session roots").raw_get(1).expect("root");
        assert_eq!(actor.get::<Function>("getaux").expect("aux getter")
            .call::<f32>(actor.clone()).expect("On result"), 1.0);
    }

    #[test]
    fn foreground_name_changes_after_init() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-foreground-name-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("test directory");
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
return Def.ActorFrame{
 Name='Declared',
 InitCommand=function(self)
  assert(self:GetName()=='Declared')
  self:name('from-init')
  assert(self:GetName()=='from-init')
 end,
 OnCommand=function(self)
  assert(self:GetName()=='scripts')
  self:aux(1)
 end,
}
"#).expect("test script");
        let mut context = SongLuaCompileContext::new(&dir, "foreground-name");
        context.live = true;
        context.layer_start_beats = vec![0.0];
        context.foreground_names = vec!["scripts".into()];
        context.music_length_seconds = 60.0;
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(
            &entry, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None,
        ).expect("foreground preparation");
        let mut scene = compiled.live_scene.take().expect("owned scene");
        let actor: Table = scene.root.as_table().expect("session roots").raw_get(1).expect("foreground root");
        assert_eq!(actor.get::<String>("Name").expect("loaded name"), "scripts");
        scene.advance(SongLuaLiveFrame::default()).expect("foreground activation");
        assert_eq!(actor.get::<Function>("getaux").expect("aux getter").call::<f32>(actor.clone()).expect("On result"), 1.0);
    }

    #[test]
    fn live_player_judgment_tree() {
        let mut compiled = compile("player-judgment-tree", r#"
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
local frame = player:GetChild('Judgment')
local sprite = frame:GetChild('JudgmentWithOffsets')
assert(frame == player:GetChild('Judgment'))
assert(sprite == frame:GetChildren().JudgmentWithOffsets)
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(self) self:aux(player:GetTweenTimeLeft()) end)
    end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        let player: Table = scene.lua.globals().get("__songlua_top_screen_player_1").expect("P1");
        let frame: Table = player.get::<Function>("GetChild").expect("child getter")
            .call((player.clone(), "Judgment")).expect("judgment frame");
        let sprite: Table = frame.get::<Function>("GetChild").expect("sprite getter")
            .call((frame.clone(), "JudgmentWithOffsets")).expect("judgment sprite");
        let mut runtime = deadsync_gameplay::init_player_runtime();
        let judgment = deadsync_rules::judgment::Judgment {
            grade: deadsync_rules::judgment::JudgeGrade::Fantastic, window: Some(deadsync_rules::judgment::TimingWindow::W0),
            time_error_ms: 0.0, time_error_music_ns: 0, miss_because_held: false,
        };
        deadsync_gameplay::set_player_last_judgment(&mut runtime, judgment, 0.125);
        let mut input = SongLuaLiveFrame { screen_seconds: 0.125, music_seconds: 0.125,
            beat: 0.25, delta_seconds: 0.125, ..Default::default() };
        input.players[0] = SongLuaLivePlayer::from_player(&runtime, 100);
        assert_eq!(input.players[0].tap_scores, [0; 11], "autoplay feedback does not require scoring");
        scene.advance(input.clone()).expect("first judgment");
        let left = |actor: &Table| crate::lua_util::actor_tween_time_left(&scene.lua, actor).expect("tween time");
        assert!((left(&sprite) - 0.9).abs() < 1e-6);
        assert!((left(&player) - 0.9).abs() < 1e-6);
        assert!((scene.states()[0].aux - 0.9).abs() < 1e-6, "authored callback sees the new queue");
        let mut pose = SongLuaOverlayState::default();
        assert!(crate::lua_util::replay_tween_pose(&scene.lua, &sprite, &mut pose));
        assert!((pose.zoom_x - 0.8).abs() < 1e-6, "new queue has not consumed this frame's delta");
        let other: Table = scene.lua.globals().get("__songlua_top_screen_player_2").expect("P2");
        assert_eq!(left(&other), 0.0, "a P1 judgment does not animate P2");

        input.screen_seconds += 0.0625;
        input.delta_seconds = 0.0625;
        scene.advance(input.clone()).expect("existing judgment advances");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &player).expect("P1 time") - 0.8375).abs() < 1e-6,
            "the named child advances once and the unchanged snapshot does not restart it");
        deadsync_gameplay::set_player_last_judgment(&mut runtime,
            deadsync_rules::judgment::Judgment { grade: deadsync_rules::judgment::JudgeGrade::Miss, window: None, ..judgment }, 0.25);
        input.players[0] = SongLuaLivePlayer::from_player(&runtime, 100);
        input.screen_seconds = 0.25;
        scene.advance(input.clone()).expect("replacement judgment");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("sprite time") - 0.9).abs() < 1e-6);
        assert_eq!(sprite.get::<Function>("GetState").expect("state getter").call::<usize>(sprite.clone()).expect("miss frame"), 5);

        player.get::<Function>("hibernate").expect("hibernate").call::<()>((player.clone(), 0.125)).expect("pause player");
        input.screen_seconds += 0.0625;
        scene.advance(input.clone()).expect("hibernating player");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("paused child") - 0.9).abs() < 1e-6);
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &player).expect("paused parent") - 0.9625).abs() < 1e-6);
        input.screen_seconds += 0.125;
        input.delta_seconds = 0.125;
        scene.advance(input.clone()).expect("wake remainder");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("wake child") - 0.8375).abs() < 1e-6);
        player.get::<Function>("SetUpdateRate").expect("update rate").call::<()>((player.clone(), 2.0)).expect("double rate");
        input.screen_seconds += 0.0625;
        input.delta_seconds = 0.0625;
        scene.advance(input.clone()).expect("rated child update");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("rated child") - 0.7125).abs() < 1e-6);
    }

    #[test]
    fn live_note_field_loads_inherited_board_sprite_outside_column_vector() {
        let compiled = compile("note-field-board", r#"
return Def.ActorFrame { OnCommand = function()
    BOARDS_CHECKED = 0
    local previous
    for index = 1, 2 do
        local field = SCREENMAN:GetTopScreen():GetChild('PlayerP' .. index):GetChild('NoteField')
        assert(field:GetNumChildren() == 1, 'NoteField must load its native Board child')
        local board = assert(field:GetChild('Board'))
        assert(board:GetName() == 'Board' and board:GetParent() == field)
        assert(field:GetChildren().Board == board and board ~= previous)
        local texture = assert(board:GetTexture(), 'Board must load the inherited blank bitmap')
        assert(texture:GetPath() == 'Themes/_fallback/Graphics/_blank.png')
        assert(texture:GetSourceWidth() == 4 and texture:GetSourceHeight() == 2)
        assert(texture:GetImageWidth() == 8 and texture:GetImageHeight() == 8)
        assert(texture:GetTextureWidth() == 8 and texture:GetTextureHeight() == 8)
        assert(texture:GetNumFrames() == 1 and board:GetWidth() == 4 and board:GetHeight() == 2)
        local columns = field:GetColumnActors()
        assert(#columns == 4)
        assert(field:GetNumChildren() == 1 and field:GetChild('Board') == board)
        assert(field:GetChild('Column1') == nil, 'column vector is separate from Board child inventory')
        previous = board
        BOARDS_CHECKED = BOARDS_CHECKED + 1
    end
end }
"#);
        let scene = compiled.live_scene.expect("native NoteField board");
        assert_eq!(scene.lua.globals().get::<i64>("BOARDS_CHECKED").unwrap(), 2,
            "regression must inspect both live NoteFields");
    }

    #[test]
    fn live_player_attack_display_preserves_native_inventory_and_hold_indices() {
        for (style, columns) in [("single", 4), ("double", 8)] {
            let source = r#"
INVENTORY_PLAYERS = {}
return Def.ActorFrame { OnCommand = function()
local screen = SCREENMAN:GetTopScreen()
for index = 1, $PLAYERS do
    local player = screen:GetChild('PlayerP' .. index)
    assert(player:GetNumChildren() == $COLUMNS + 4,
        'visible Player must include its native AttackDisplay')
    local children = player:GetChildren()['']
    local named = player:GetChild('')
    assert(#children == $COLUMNS + 1 and #named == #children)
    local attack = children[1]
    assert(attack:GetName() == '' and attack:GetParent() == player)
    assert(tostring(attack):find('ActorFrame', 1, true) and attack:GetNumChildren() == 0)
    assert(attack:GetX() == -40 and attack:GetY() == 0 and attack:GetTweenTimeLeft() == 0)
    for index = 1, #children do
        assert(named[index] == children[index], 'lookups share the same unnamed actors')
        if index > 1 then
            local hold = children[index]
            assert(tostring(hold):find('HoldJudgment', 1, true))
            assert(hold:GetParent() == player and hold:GetNumChildren() == 1)
            assert(hold:GetChild('HoldJudgment'):GetParent() == hold)
        end
    end
    table.insert(INVENTORY_PLAYERS, player)
end
end }
"#.replace("$COLUMNS", &columns.to_string())
                .replace("$PLAYERS", if style == "single" { "2" } else { "1" });
            let compiled = compile_with_context(&format!("player-attack-display-{style}"), &source, |context| {
                context.style_name = style.into();
                context.players[1].enabled = style == "single";
            });
            let scene = compiled.live_scene.expect("native Player inventory");
            let players = scene.lua.globals().get::<Table>("INVENTORY_PLAYERS").unwrap();
            assert_eq!(players.raw_len(), if style == "single" { 2 } else { 1 },
                "regression must inspect every configured visible player");
            for player in players.sequence_values::<Table>() {
                let player = player.unwrap();
                let children = crate::lua_util::actor_direct_children(&scene.lua, &player).unwrap();
                let mut expected = vec!["", "Combo", "Judgment"];
                expected.extend(std::iter::repeat_n("", columns));
                expected.push("NoteField");
                assert_eq!(children.iter().map(|child| crate::lua_util::actor_name(child).unwrap())
                    .collect::<Vec<_>>(), expected, "native constructor and Init child order");
                let holds = player.raw_get::<Table>("__songlua_hold_judgments").unwrap();
                assert_eq!(holds.raw_len(), columns, "feedback retains exactly one frame per column");
                for column in 0..columns {
                    assert_eq!(holds.raw_get::<Table>(column + 1).unwrap().to_pointer(),
                        children[column + 3].to_pointer(), "attack display must not shift hold indices");
                }
            }
        }
    }

    #[test]
    fn live_player_combo_uses_current_theme_font_resources() {
        for populated in [false, true] {
            let source = r#"
COMBOS = {}
return Def.Actor { OnCommand = function()
    for index = 1, 2 do
        local player = SCREENMAN:GetTopScreen():GetChild('PlayerP' .. index)
        local combo = player:GetChild('Combo')
        assert(combo:GetParent() == player and combo:GetName() == 'Combo')
        if $POPULATED then
            assert(tostring(combo):find('ActorFrame', 1, true))
            local number = combo:GetChild('Number')
            assert(tostring(number):find('BitmapText', 1, true))
            assert(number:GetParent() == combo and combo:GetNumChildren() == 3)
            local hundred = combo:GetChild('OneHundredMilestone')
            local thousand = combo:GetChild('OneThousandMilestone')
            assert(hundred:GetNumChildren() == 4 and thousand:GetNumChildren() == 3)
            assert(hundred:GetParent() == combo and thousand:GetParent() == combo)
            assert(hundred:GetVisible() == false and thousand:GetVisible() == false)
            local nested = thousand:GetChildren()[''][1]
            assert(nested:GetNumChildren() == 4 and nested:GetParent() == thousand)
            for _, sprite in ipairs(hundred:GetChildren()['']) do
                assert(tostring(sprite):find('Sprite', 1, true) and sprite:GetRotationZ() == 0)
                assert(sprite:GetDiffuseAlpha() == 0)
            end
            hundred:playcommand('HundredMilestone')
            assert(hundred:GetVisible() == true and hundred:GetTweenTimeLeft() > 0)
            hundred:playcommand('Hide')
            assert(hundred:GetVisible() == false)
            assert(combo:GetChildren().Number == number)
        else
            assert(not tostring(combo):find('ActorFrame', 1, true),
                'Combo without current-theme fonts must be a leaf Actor')
            assert(combo:GetVisible() == false and combo.GetNumChildren == nil)
        end
        assert(SL['P' .. index].ActiveModifiers.HideCombo == false)
        COMBOS[index] = combo
    end
end }
"#.replace("$POPULATED", if populated { "true" } else { "false" });
            let compiled = compile_with_context(
                &format!("combo-current-theme-resources-{populated}"), &source, |context| {
                    let fonts = context.song_dir.join(crate::SONG_LUA_THEME_PATH_PREFIX)
                        .join("Fonts/_Combo Fonts");
                    if populated {
                        for name in ["Asap", "Wendy", "Wendy (Cursed)", "MissingIni", "MissingPng", "WrongName"] {
                            let folder = fonts.join(name);
                            fs::create_dir_all(&folder).unwrap();
                            if name != "MissingIni" {
                                fs::write(folder.join(format!("{name}.ini")), "[main]\n").unwrap();
                            }
                            if name != "MissingPng" {
                                let stem = if name == "WrongName" { "Other" } else { name };
                                fs::write(folder.join(format!("{stem} 4x4.png")), b"").unwrap();
                            }
                        }
                    }
                });
            let scene = compiled.live_scene.expect("owned Combo resource selection");
            let combos = scene.lua.globals().get::<Table>("COMBOS").unwrap();
            assert_eq!(combos.raw_len(), 2, "inspect both configured players after On");
            assert_ne!(combos.raw_get::<Table>(1).unwrap().to_pointer(),
                combos.raw_get::<Table>(2).unwrap().to_pointer());
            let fonts = scene.lua.globals().get::<Function>("GetComboFonts").unwrap()
                .call::<Table>(()).unwrap().sequence_values::<String>()
                .map(Result::unwrap).collect::<Vec<_>>();
            assert_eq!(fonts, if populated { vec!["Wendy", "Asap", "Wendy (Cursed)"] } else { vec![] });
        }
    }

    #[test]
    fn live_player_hold_judgment_tree() {
        let mut compiled = compile("player-hold-judgment-tree", r#"
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
local unnamed = player:GetChildren()['']
local named = player:GetChild('')
assert(#unnamed == 5 and #named == 5 and named[1] == unnamed[1])
assert(player:GetNumChildren() == 8)
assert(unnamed[1]:GetNumChildren() == 0 and unnamed[1]:GetParent() == player)
local holds = {unnamed[2], unnamed[3], unnamed[4], unnamed[5]}
assert(#holds == 4 and named[2] == holds[1])
assert(tostring(holds[1]):find('HoldJudgment', 1, true))
for _, frame in ipairs(holds) do
    assert(frame:GetChild('HoldJudgment'):GetParent() == frame)
end
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(self) self:aux(player:GetTweenTimeLeft()) end)
    end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        let player: Table = scene.lua.globals().get("__songlua_top_screen_player_1").expect("P1");
        let other: Table = scene.lua.globals().get("__songlua_top_screen_player_2").expect("P2");
        let frames: Table = player.raw_get("__songlua_hold_judgments").expect("column judgments");
        let frame: Table = frames.raw_get(2).expect("second column");
        let sprite: Table = crate::lua_util::actor_children(&scene.lua, &frame).expect("frame children")
            .raw_get("HoldJudgment").expect("native sprite");
        let mut input = SongLuaLiveFrame { screen_seconds: 0.125, delta_seconds: 0.125, ..Default::default() };
        input.players[0].hold_judgments[1] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
            result: deadsync_rules::note::HoldResult::Held, started_at_screen_s: 0.125,
        });
        scene.advance(input.clone()).expect("held feedback without tap feedback or scoring");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("hold queue") - 0.8).abs() < 1e-6);
        assert!((scene.states()[0].aux - 0.8).abs() < 1e-6, "authored callbacks see hold descendants");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &other).expect("P2 queue"), 0.0);
        let mut pose = SongLuaOverlayState::default();
        assert!(crate::lua_util::replay_tween_pose(&scene.lua, &sprite, &mut pose));
        assert!((pose.zoom_x - 0.4).abs() < 1e-6, "new queue does not consume the current frame");
        assert_eq!(pose.diffuse[3], 1.0);

        input.screen_seconds += 0.0625;
        input.delta_seconds = 0.0625;
        scene.advance(input.clone()).expect("unchanged hold feedback");
        assert!((scene.states()[0].aux - 0.7375).abs() < 1e-6, "the snapshot does not restart the queue");
        input.players[0].hold_judgments[1] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
            result: deadsync_rules::note::HoldResult::LetGo, started_at_screen_s: 0.25,
        });
        scene.advance(input.clone()).expect("let-go replaces the old queue");
        assert!((scene.states()[0].aux - 0.8).abs() < 1e-6);
        assert_eq!(sprite.get::<Function>("GetState").expect("state getter")
            .call::<usize>(sprite.clone()).expect("let-go frame"), 1);
        input.players[0].hold_judgments[1] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
            result: deadsync_rules::note::HoldResult::Missed, started_at_screen_s: 0.3125,
        });
        scene.advance(input.clone()).expect("native empty MissedHold command");
        assert!((scene.states()[0].aux - 0.7375).abs() < 1e-6);
        input.players[0].hold_judgments.fill(None);
        input.delta_seconds = 1.0;
        scene.advance(input.clone()).expect("feedback expires independently of the snapshot");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &player).expect("finished player"), 0.0);
    }

    #[test]
    fn live_note_field_tap_returns_follow_native_optional_conversions() {
        let lua = Lua::new();
        let field = lua.create_table().unwrap();
        lua.set_app_data(QueryTrace::default());
        lua.globals().set("CommandDepth", lua.create_function(|lua, ()| {
            Ok(lua.app_data_ref::<QueryTrace>().expect("observed command scope").commands)
        }).unwrap()).unwrap();
        let original = deadsync_gameplay::GameplayNoteFieldTap {
            player: 1, column: 3, score: "W3", bright: false,
        };
        for (body, column, score, bright) in [
            ("return nil, nil, nil", 3, "W3", false),
            ("return '2.8', 'TapNoteScore_W1', true", 1, "W1", true),
            ("return 0.5, 'w2', 1", 0, "W2", false),
            ("return false, 9, true", 3, "W1", true),
        ] {
            let callback: Function = lua.load(format!(r#"
return function(column, score, bright)
    assert(CommandDepth() == 0, 'direct NoteField callback has no ActorFrame command scope')
    assert(column == 4 and score == 'TapNoteScore_W3' and bright == false)
    {body}
end
"#)).eval().unwrap();
            let actual = invoke_note_field_tap_callback(&lua, &field, &callback, original, 4).unwrap();
            assert_eq!(actual, deadsync_gameplay::GameplayNoteFieldTap { column, score, bright, ..original });
        }
        let invalid: Function = lua.load("return function() return 9, 'typo', true end").eval().unwrap();
        let actual = invoke_note_field_tap_callback(&lua, &field, &invalid, original, 4).unwrap();
        assert_eq!(actual, deadsync_gameplay::GameplayNoteFieldTap { bright: true, ..original });
        assert!(field.raw_get::<bool>("__songlua_did_tap_note_column_error_reported").unwrap());
        assert!(field.raw_get::<bool>("__songlua_did_tap_note_score_error_reported").unwrap());
        let fails: Function = lua.load("return function() error('authored tap failure') end").eval().unwrap();
        assert!(invoke_note_field_tap_callback(&lua, &field, &fails, original, 4).is_err());
        assert_eq!(lua.app_data_ref::<QueryTrace>().unwrap().commands, 0);
        lua.app_data_mut::<QueryTrace>().unwrap().commands = 2;
        let nested: Function = lua.load("return function() assert(CommandDepth() == 2); return nil end").eval().unwrap();
        assert_eq!(invoke_note_field_tap_callback(&lua, &field, &nested, original, 4).unwrap(), original);
        assert_eq!(lua.app_data_ref::<QueryTrace>().unwrap().commands, 2,
            "the direct call preserves an already active authored scope");
    }

    #[test]
    fn live_color_constants_preserve_native_palette_bound() {
        let mut compiled = compile("native-color-palette", r#"
assert(Color.Pink == nil, 'native Color table does not define Pink')
assert(Color('Pink') == nil and Color.NotAColor == nil)
assert(Color('White') == Color.White)
local palette = {
    Color.White, Color.Yellow, Color.Blue, Color.Red, Color.White,
    Color.Purple, Color.Green, Color.Orange, Color.Pink,
}
assert(#palette == 8, 'trailing native nil leaves an eight-entry palette')
math.randomseed(1)
local expected = {4, 8, 6, 8, 1, 2, 3, 8}
return Def.ActorFrame {
    InitCommand = function(self)
        for _, index in ipairs(expected) do
            local selected = math.random(#palette)
            assert(selected == index, 'native palette preserves seeded random choices')
            self:diffuse(palette[selected])
        end
        PALETTE_CHECKED = true
    end,
}
"#);
        let scene = compiled.live_scene.take().expect("production color scene");
        assert!(scene.lua.globals().get::<bool>("PALETTE_CHECKED").unwrap());
        assert_eq!(scene.states()[0].diffuse, [247.0 / 255.0, 148.0 / 255.0, 29.0 / 255.0, 1.0]);
    }

    #[test]
    fn live_named_notefield_advances_once() {
        let mut compiled = compile("named-notefield-once", r#"
local field = SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
FIELD = field
return Def.ActorFrame {
    OnCommand = function(self)
        field:sleep(0.5)
        self:SetUpdateFunction(function(self) self:aux(field:GetTweenTimeLeft()) end)
    end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("owned scene");
        scene.advance(SongLuaLiveFrame { screen_seconds: 0.125, delta_seconds: 0.125,
            ..Default::default() }).expect("one native child update");
        assert!((scene.states()[0].aux - 0.375).abs() < 1e-6, "NoteField does not receive an extra special update");
        scene.lua.load(r#"
local before = FIELD:GetNumChildren()
COL = FIELD:GetColumnActors()[2]
assert(COL == FIELD:get_column_actors()[2])
assert(FIELD:GetNumChildren() == before, 'column vector is separate from ActorFrame children')
COL:x(0):linear(1):x(100):queuecommand('ColumnDone')
COL:addcommand('ColumnDone', function(self) COLUMN_DONE = (COLUMN_DONE or 0) + 1 end)
FIELD:SetUpdateFunction(function() COLUMN_BEFORE = COL:GetTweenTimeLeft() end)
"#).exec().expect("owned column commands");
        let column: Table = scene.lua.globals().get("COL").unwrap();
        let left = |actor: &Table| crate::lua_util::actor_tween_time_left(&scene.lua, actor).unwrap();
        assert_eq!(left(&column), 1.0);
        let mut input = SongLuaLiveFrame { screen_seconds: 0.25, delta_seconds: 0.125,
            ..Default::default() };
        scene.advance(input.clone()).expect("column update after the field callback");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.875,
            "NoteField's owned column vector advances once per screen update");
        assert_eq!(scene.lua.globals().get::<f32>("COLUMN_BEFORE").unwrap(), 1.0);
        let x = column.get::<Function>("GetX").unwrap().call::<f32>(column.clone()).unwrap();
        assert_eq!(x, 12.5, "the real column tween updates its authored pose");
        input.delta_seconds = 0.0;
        scene.advance(input.clone()).expect("zero delta preserves the column queue");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.875);
        scene.lua.load("FIELD:SetUpdateRate(3); WRAP = COL:AddWrapperState():sleep(1)")
            .exec().expect("field rate and column wrapper");
        let wrapper: Table = scene.lua.globals().get("WRAP").unwrap();
        input.delta_seconds = 0.125;
        input.screen_seconds += 0.125;
        scene.advance(input.clone()).expect("columns use the field's original delta");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.75);
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &wrapper).unwrap(), 0.875);
        scene.lua.load("FIELD:hibernate(0.25)").exec().unwrap();
        input.screen_seconds += 0.125;
        scene.advance(input.clone()).expect("NoteField still updates columns while its ActorFrame sleeps");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.625);
        let player: Table = scene.lua.globals().get("__songlua_top_screen_player_1").unwrap();
        player.get::<Function>("hibernate").unwrap().call::<()>((player.clone(), 0.25)).unwrap();
        for _ in 0..2 {
            input.screen_seconds += 0.125;
            scene.advance(input.clone()).expect("hibernating ancestor skips the field; wake has zero overshoot");
            assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.625);
        }
        input.screen_seconds += 0.125;
        scene.advance(input.clone()).expect("field wake and column vector receive the original delta");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.5);
        player.get::<Function>("SetUpdateRate").unwrap().call::<()>((player.clone(), 2.0)).unwrap();
        input.screen_seconds += 0.125;
        scene.advance(input.clone()).expect("column vector inherits its ancestor's rate");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.25);
        column.get::<Function>("hibernate").unwrap().call::<()>((column.clone(), 0.125)).unwrap();
        input.screen_seconds += 0.125;
        scene.advance(input.clone()).expect("column hibernation passes only its wake overshoot to wrappers");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.125);
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &wrapper).unwrap(), 0.25);
        for _ in 0..2 {
            input.screen_seconds += 0.125;
            scene.advance(input.clone()).expect("column tween and queued command complete");
        }
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &column).unwrap(), 0.0);
        assert_eq!(column.get::<Function>("GetX").unwrap().call::<f32>(column).unwrap(), 100.0);
        assert_eq!(scene.lua.globals().get::<u32>("COLUMN_DONE").unwrap(), 1);
    }

    #[test]
    fn live_foreground_clock() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-foreground-clock-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("test directory");
        let early = dir.join("early.lua");
        let late = dir.join("late.lua");
        fs::write(dir.join("clock.lua"), r#"
local label = ...
return Def.ActorFrame {
    Name = label,
    InitCommand = function() MESSAGEMAN:Broadcast('Construct:' .. label) end,
    OnCommand = function(self) self:sleep(999); MESSAGEMAN:Broadcast('On:' .. label) end,
    Def.Actor {
        OnCommand = function(self) self:sleep(0.25):queuecommand('Later') end,
        LaterCommand = function() MESSAGEMAN:Broadcast('Later:' .. label) end,
    },
}
"#).expect("clock script");
        fs::write(&early, "return LoadActor('clock.lua', 'early')").expect("early entry");
        fs::write(&late, "return LoadActor('clock.lua', 'late')").expect("late entry");
        let mut context = SongLuaCompileContext::new(&dir, "foreground clock");
        context.live = true;
        context.song_display_bpms = [120.0; 2];
        context.layer_start_beats = vec![2.0, 4.0];
        let mut compiled: Vec<Compiled> = crate::compile_song_lua_layers_with_default_host(&[&early, &late], 0,
            &context, SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None)
            .expect("scheduled preparation");
        let mut scene = compiled[0].live_scene.take().expect("owned scheduled scene");
        let startup = scene.take_startup_output();
        let names = startup.broadcasts.iter().map(|(_, name, _)| name.as_str()).collect::<Vec<_>>();
        assert!(names.contains(&"Construct:early") && names.contains(&"Construct:late"));
        assert!(!names.iter().any(|name| name.starts_with("On:") || name.starts_with("Later:")));
        for (index, (music_seconds, expected)) in [
            (0.75, None), (1.0, Some("On:early")), (1.0, None),
            (1.125, None), (1.25, None), (1.375, Some("Later:early")),
            (2.0, Some("On:late")),
        ].into_iter().enumerate() {
            let output = scene.advance(SongLuaLiveFrame { music_seconds, beat: music_seconds * 2.0,
                screen_seconds: (index + 1) as f64 * 0.5, delta_seconds: 0.5, ..Default::default() }).expect("scheduled frame");
            assert!(output.errors.is_empty());
            let names = output.broadcasts.iter().map(|(_, name, _)| name.as_str()).collect::<Vec<_>>();
            assert_eq!(names, expected.into_iter().collect::<Vec<_>>(), "music second {music_seconds}");
        }
    }

    #[test]
    fn live_background_starts_at_change_beat() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-background-start-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("test directory");
        let entry = dir.join("background.lua");
        fs::write(&entry, r#"
return Def.ActorFrame {
    OnCommand = function(self)
        MESSAGEMAN:Broadcast('On:' .. tostring(GAMESTATE:GetCurMusicSeconds()))
        self:SetUpdateFunction(function(self, delta)
            MESSAGEMAN:Broadcast('Delta:' .. tostring(delta))
        end)
    end,
    GainFocusCommand = function() MESSAGEMAN:Broadcast('GainFocus') end,
    Def.Actor {
        OnCommand = function(self) self:sleep(0.1):queuecommand('Later') end,
        LaterCommand = function() MESSAGEMAN:Broadcast('Later') end,
    },
}
"#).expect("background script");
        let mut context = SongLuaCompileContext::new(&dir, "background start");
        context.live = true;
        context.song_display_bpms = [120.0; 2];
        context.background_layer_count = 1;
        context.layer_start_beats = vec![2.0];
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("background preparation");
        let mut scene = compiled.live_scene.take().expect("owned scene");
        assert!(scene.take_startup_output().broadcasts.iter().all(|(_, name, _)| name == "ScreenChanged"));
        let dormant = scene.advance(SongLuaLiveFrame { music_seconds: 0.75, beat: 1.5,
            screen_seconds: 4.5, delta_seconds: 0.5, ..Default::default() }).expect("dormant frame");
        assert!(dormant.broadcasts.is_empty());
        for (music_seconds, expected) in [(1.125, vec!["On:1.125", "GainFocus", "Later", "Delta:0.125"]),
            (1.25, vec!["Delta:0.125"])] {
            let output = scene.advance(SongLuaLiveFrame { music_seconds, beat: music_seconds * 2.0,
                screen_seconds: 5.0, delta_seconds: 0.5, ..Default::default() }).expect("background frame");
            assert!(output.errors.is_empty());
            assert_eq!(output.broadcasts.iter().map(|(_, name, _)| name.as_str()).collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn live_foreground_late_start() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-foreground-late-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("test directory");
        let entry = dir.join("late.lua");
        fs::write(&entry, r#"
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(999):SetUpdateFunction(function(self, delta)
            MESSAGEMAN:Broadcast('Delta:' .. tostring(delta))
        end)
    end,
    Def.Actor {
        OnCommand = function(self) self:sleep(0.1):queuecommand('Later') end,
        LaterCommand = function() MESSAGEMAN:Broadcast('Later') end,
    },
}
"#).expect("late script");
        let mut context = SongLuaCompileContext::new(&dir, "foreground late");
        context.live = true;
        context.song_display_bpms = [120.0; 2];
        context.layer_start_beats = vec![2.0];
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("late preparation");
        let mut scene = compiled.live_scene.take().expect("owned scene");
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 1.125, beat: 2.25,
            screen_seconds: 5.0, delta_seconds: 0.5, ..Default::default() }).expect("late frame");
        assert!(output.errors.is_empty());
        assert_eq!(output.broadcasts.iter().map(|(_, name, _)| name.as_str()).collect::<Vec<_>>(),
            ["Later", "Delta:0.125"]);
    }

    #[test]
    fn live_foreground_finishes() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-foreground-finish-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("test directory");
        let entry = dir.join("finish.lua");
        fs::write(&entry, r#"
local count = 0
return Def.ActorFrame {
    Name = 'Finish',
    InitCommand = function(self) self:SetUpdateFunction(function(self)
        count = count + 1; MESSAGEMAN:Broadcast('ForegroundTick' .. count)
    end) end,
    OnCommand = function(self) self:sleep(0.25) end,
}
"#).expect("finish script");
        let mut context = SongLuaCompileContext::new(&dir, "foreground finish");
        context.live = true;
        context.song_display_bpms = [120.0; 2];
        context.layer_start_beats = vec![2.0];
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("finish preparation");
        let global = compiled.live_actor_indices()[compiled.overlays.iter()
            .position(|actor| actor.name.as_deref() == Some("Finish")).expect("finish actor")];
        let mut scene = compiled.live_scene.take().expect("owned scene");
        assert!(!scene.states()[global].visible);
        assert!(scene.take_startup_output().broadcasts.iter().all(|(_, name, _)| !name.starts_with("ForegroundTick")),
            "dormant callbacks do not run in preparation");
        for (index, (music_seconds, tick, visible)) in [(0.75, None, false), (1.0, Some("ForegroundTick1"), true),
            (1.5, Some("ForegroundTick2"), false), (1.75, None, false), (1.0, None, false)].into_iter().enumerate() {
            let output = scene.advance(SongLuaLiveFrame { music_seconds, beat: music_seconds * 2.0,
                screen_seconds: (index + 1) as f64 * 0.5, delta_seconds: 0.5, ..Default::default() }).expect("finish frame");
            assert!(output.errors.is_empty());
            assert_eq!(output.broadcasts.iter().map(|(_, name, _)| name.as_str()).collect::<Vec<_>>(), tick.into_iter().collect::<Vec<_>>());
            assert_eq!(scene.states()[global].visible, visible);
        }
    }

    #[test]
    fn live_queue() {
        let mut compiled = std::thread::spawn(|| compile("queue", r#"
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local count = 0
return Def.ActorFrame {
    Name = 'Moving',
    OnCommand = function(self) self:x(0):linear(1):x(100):queuecommand('Later') end,
    LaterCommand = function(self)
        count = count + 1
        self:aux(stats:GetCurrentPossibleDancePoints())
        MESSAGEMAN:Broadcast('ActualTick', { count = count })
    end,
    UnsolicitedMessageCommand = function(self) error('speculative listener called') end,
}
"#)).join().expect("worker transfers the initialized VM");
        let global = compiled.live_actor_indices()[compiled.overlays.iter()
            .position(|actor| actor.name.as_deref() == Some("Moving")).expect("moving actor")];
        let mut scene = compiled.live_scene.take().expect("owned scene");
        assert_eq!(scene.states()[global].x, 0.0, "startup must retain current pose, not destination");
        assert!(scene.take_startup_output().errors.is_empty());
        let mut player = deadsync_gameplay::init_player_runtime();
        player.current_possible_points = 5;
        let mut frame = SongLuaLiveFrame { screen_seconds: 0.5, delta_seconds: 0.5,
            players: [SongLuaLivePlayer::from_player(&player, 10), SongLuaLivePlayer::default()],
            ..SongLuaLiveFrame::default() };
        let output = scene.advance(frame.clone()).expect("first half frame");
        assert!(output.errors.is_empty() && output.broadcasts.is_empty());
        assert!((scene.states()[global].x - 50.0).abs() < 1e-5);
        player.current_possible_points = 10;
        frame.players[0] = SongLuaLivePlayer::from_player(&player, 10);
        frame.screen_seconds = 1.0;
        let output = scene.advance(frame.clone()).expect("queue completion");
        assert!(output.errors.is_empty() && output.broadcasts.is_empty());
        assert_eq!(scene.states()[global].x, 100.0);
        // Actor.cpp::UpdateTweening stops when the positive delta is exhausted.
        // A trailing zero-time command runs on the next positive update.
        frame.screen_seconds = 1.1;
        frame.delta_seconds = 0.1;
        let output = scene.advance(frame.clone()).expect("trailing command");
        assert!(output.errors.is_empty());
        assert_eq!(output.broadcasts.len(), 1);
        assert_eq!(output.broadcasts[0].1, "ActualTick");
        assert_eq!(scene.states()[global].aux, 10.0);
        frame.screen_seconds = 1.2;
        assert!(scene.advance(frame.clone()).expect("later frame").broadcasts.is_empty(), "drain events exactly once");
    }

    #[test]
    fn live_uptime_advances_while_music_is_paused() {
        let mut compiled = compile("uptime-paused", r#"
local last = nil
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        local now = GetTimeSinceStart()
        if now == 0 then return end
        if last == nil then
            last = now
            SCREENMAN:GetTopScreen():PauseGame(true)
        elseif now - last > 0.25 then
            SCREENMAN:GetTopScreen():PauseGame(false)
        end
        CLOCK = {now, GAMESTATE:GetCurMusicSeconds(), SCREENMAN:GetTopScreen():IsPaused()}
    end)
end }
"#);
        let mut scene = compiled.live_scene.take().expect("scene");
        for (uptime, paused) in [(40.1250001, true), (40.25, true), (40.5, false)] {
            let output = scene.advance(SongLuaLiveFrame { music_seconds: 12.0,
                screen_seconds: 1.0, delta_seconds: 0.125,
                uptime_seconds: Some(uptime), ..Default::default() }).expect("paused clock frame");
            assert!(output.errors.is_empty());
            let clock = scene.lua.globals().get::<Table>("CLOCK").expect("actual clock queries");
            assert_eq!(clock.get::<f64>(1).expect("double process time"), uptime);
            assert_eq!(clock.get::<f64>(2).expect("frozen music time"), 12.0);
            assert_eq!(clock.get::<bool>(3).expect("pause state"), paused);
        }
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.0,
            screen_seconds: 1.125, delta_seconds: 0.125, ..Default::default() }).expect("fallback elapsed clock");
        assert!(output.errors.is_empty());
        let clock = scene.lua.globals().get::<Table>("CLOCK").expect("fallback clock queries");
        assert_eq!(clock.get::<f64>(1).expect("process clock retains its epoch"), 40.625);
    }

    #[test]
    fn live_song_clock() {
        let mut compiled = compile("song-clock", r#"
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        CLOCK = {GAMESTATE:GetSongBeat(), GAMESTATE:GetSongPosition():GetSongBeat(),
            GAMESTATE:GetCurMusicSeconds(), GAMESTATE:GetSongBPS(), GAMESTATE:GetSongPosition():GetCurBPS()}
    end)
end }
"#);
        let mut scene = compiled.live_scene.take().expect("scene");
        // Native SongPosition adds the machine offset and group sync offset.
        // A 120 BPM chart has different beats from this 60 BPM song clock.
        scene.context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
            0.009, -0.008, &deadsync_rules::timing::TimingSegments {
                bpms: vec![(0.0, 60.0), (2.0, 120.0)], ..Default::default()
            }, &[]));
        scene.lua.set_app_data(crate::runtime::SongLuaClock(
            scene.context.song_timing.clone().expect("global song timing")));
        for (seconds, player_beat, expected, bps) in [(1.017, 2.0, 1.0, 1.0), (3.017, 6.0, 4.0, 2.0)] {
            let output = scene.advance(SongLuaLiveFrame { beat: player_beat, music_seconds: seconds,
                screen_seconds: seconds, delta_seconds: 0.1, ..Default::default() }).expect("live frame");
            assert!(output.errors.is_empty());
            let values = scene.lua.globals().get::<Table>("CLOCK").expect("actual Lua queries");
            for index in [1, 2] {
                let actual = values.get::<f64>(index).expect("song beat");
                assert!((actual - expected).abs() < 1e-5,
                    "query {index}: seconds={seconds} song={actual} player={player_beat} expected={expected}");
            }
            assert!((values.get::<f64>(3).expect("music seconds") - seconds).abs() < 1e-5);
            for index in [4, 5] { assert_eq!(values.get::<f64>(index).expect("current BPS"), bps); }
        }
    }

    #[test]
    fn live_deprecated_hidden_preserves_lua_truthiness_and_reports_warning() {
        compile("deprecated-hidden", r#"
local warnings = 0
return Def.ActorFrame { Name='hidden-check',
    ScriptErrorMessageCommand=function(self, params)
        assert(params.message == 'hidden is deprecated, use visible instead. (used on hidden-check)')
        warnings = warnings + 1
    end,
    OnCommand=function(self)
        assert(self:hidden(true) == nil and not self:GetVisible())
        assert(self:hidden(false) == nil and self:GetVisible())
        assert(self:hidden(0) == nil and not self:GetVisible())
        assert(self:hidden(nil) == nil and self:GetVisible())
        assert(warnings == 4)
    end,
}
"#);
    }

    #[test]
    fn live_native_tween_curves_match_native_probes() {
        let mut compiled = compile_with_context("native-tween-curves", r#"
ACTORS = {}
local function actor(name, kind, curve)
    return Def.Actor { Name=name,
        InitCommand=function(self) ACTORS[name]=self;self:x(0) end,
        OnCommand=function(self)
            if name == 'drop' then
                assert(self:drop(1) == self)
                self:x(100)
            else self:tween(1,kind,curve):x(100) end
        end }
end
return Def.ActorFrame { InitCommand=function(self) TWEEN_ROOT=self end,
    actor('linear',0,0), actor('accelerate','TweenType_Accelerate'),
    actor('decelerate','DECELERATE'), actor('spring',3),
    actor('bezier4',4,{0.2,-0.3,1.4,1.3}),
    actor('bezier8','TweenType_Bezier',{-0.2,0.1,0.1,-0.4,0.85,1.4,1.2,0.9}),
    actor('drop') }
"#, |context| context.defer_initial_live_update = true);
        let mut scene = compiled.live_scene.take().unwrap();
        let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
        assert!(output.errors.is_empty(), "native tween callbacks: {:?}", output.errors);
        scene.lua.load(r#"
TWEEN_ROOT:hurrytweening(2)
for _,a in pairs(ACTORS) do assert(a:GetTweenTimeLeft()==2) end
TWEEN_ROOT:hurrytweening(0.5)
for _,a in pairs(ACTORS) do assert(a:GetTweenTimeLeft()==1) end
"#).exec().unwrap();
        // ITGmania actor-conformance curve_probe values, generated by the
        // native ITween factory rather than the owned curve evaluator.
        let probes = [
            ("linear", [0.0, 0.25, 0.5, 0.75, 1.0]),
            ("accelerate", [0.0, 0.0625, 0.25, 0.5625, 1.0]),
            ("decelerate", [0.0, 0.4375, 0.75, 0.9375, 1.0]),
            ("spring", [0.0, 1.2186763, 1.2828426, 0.71572936, 1.0000001]),
            ("bezier4", [0.2, 0.17500003, 0.6000001, 1.0999999, 1.3]),
            ("bezier8", [0.026412196, 0.22954142, 0.5226672, 0.80551004, 0.98091364]),
            ("drop", [0.0, 0.5078125, 0.6875, 0.7734375, 1.0]),
        ];
        for step in 1..=3 {
            let seconds = f64::from(step) * 0.25;
            let output = scene.advance(SongLuaLiveFrame { beat: seconds,
                music_seconds: seconds, screen_seconds: seconds, delta_seconds: 0.25,
                ..Default::default() }).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let actors = scene.lua.globals().get::<Table>("ACTORS").unwrap();
            for (name, expected) in probes {
                let actor = actors.get::<Table>(name).unwrap();
                let actual = actor.get::<Function>("GetX").unwrap().call::<f32>(actor).unwrap();
                assert!((actual - expected[step as usize] * 100.0).abs() < 0.0001,
                    "native curve {name} at {seconds}: {actual}");
            }
        }
        for (curve, expected) in [
            ("bezier(0.2,-0.3,1.4,1.3)", probes[4].1),
            ("bezier(-0.2,0.1,0.1,-0.4,0.85,1.4,1.2,0.9)", probes[5].1),
            ("bezier(0,0,0.33333334,1,0.6666667,0.5,1,1)", probes[6].1),
        ] {
            for (index, expected) in expected.into_iter().enumerate() {
                let factor = deadsync_gameplay::song_lua_ease_factor(
                    Some(curve), index as f32 * 0.25, None, None);
                assert!((factor - expected).abs() < 0.000001, "native probe {curve}: {factor}");
            }
        }
        scene.lua.load(r#"
local a=ACTORS.linear
a:hurrytweening(0.5)
assert(a:GetTweenTimeLeft()==0.125)
assert(a:GetX()==75, 'hurry retains the current interpolation fraction')
"#).exec().unwrap();
        scene.advance(SongLuaLiveFrame { beat: 0.8125, music_seconds: 0.8125,
            screen_seconds: 0.8125, delta_seconds: 0.0625, ..Default::default() }).unwrap();
        scene.lua.load(r#"
local a=ACTORS.linear
assert(a:GetX()==87.5)
a:hurrytweening(0)
assert(a:GetTweenTimeLeft()==0 and a:GetX()==87.5)
assert(not pcall(function() a:hurrytweening(-1) end))
"#).exec().unwrap();
        scene.advance(SongLuaLiveFrame { beat: 0.8225, music_seconds: 0.8225,
            screen_seconds: 0.8225, delta_seconds: 0.01, ..Default::default() }).unwrap();
        scene.lua.load("assert(ACTORS.linear:GetX()==100)").exec().unwrap();
    }

    #[test]
    fn live_native_tween_argument_and_hurry_contract() {
        let mut compiled = compile_with_context("native-tween-arguments", r#"
return Def.Actor { OnCommand=function(a)
    for _,kind in ipairs({0,1,2,3,'TweenType_Linear','accelerate','DECELERATE','Spring'}) do
        a:stoptweening()
        assert(a:tween('2',kind,0)==a)
        assert(a:GetTweenTimeLeft()==2)
        a:hurrytweening(0.25)
        assert(a:GetTweenTimeLeft()==0.5)
    end
    for _,curve in ipairs({{0.2,-0.3,1.4,1.3},
        {-0.2,0.1,0.1,-0.4,0.85,1.4,1.2,0.9},{false,'0',nil,1}}) do
        a:stoptweening();assert(a:tween(2,'TweenType_Bezier',curve)==a)
        assert(a:GetTweenTimeLeft()==2)
    end
    a:stoptweening()
    assert(not pcall(function() a:tween(1,1.5) end))
    assert(not pcall(function() a:tween(1,'bad') end))
    assert(not pcall(function() a:tween(1,'TweenType_Bezier',false) end))
    assert(a:GetTweenTimeLeft()==0)
    assert(a:tween(-1,'bad')==a and a:GetTweenTimeLeft()==0)
    assert(a:tween(1,'TweenType_Bezier',{0,1})==a and a:GetTweenTimeLeft()==0)
    a:hibernate(10):tween(2,0,0):hurrytweening(0.25)
    assert(a:GetTweenTimeLeft()==10.5, 'hurry must not scale hibernation')
    CONTRACT_PASSED = true
end }
"#, |context| context.defer_initial_live_update = true);
        let mut scene = compiled.live_scene.take().unwrap();
        let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
        assert!(output.errors.is_empty(), "native tween arguments: {:?}", output.errors);
        assert!(scene.lua.globals().get::<bool>("CONTRACT_PASSED").unwrap());
    }

    #[test]
    fn live_player_song_positions_use_selected_chart_timing() {
        use deadsync_rules::timing::{DelaySegment, StopSegment, TimingData, TimingSegments, WarpSegment};
        let mut compiled = compile_with_context("player-song-positions", r#"
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        POSITIONS = {}
        for _, player in ipairs({PLAYER_1, PLAYER_2}) do
            local position = GAMESTATE:GetPlayerState(player):GetSongPosition()
            POSITIONS[#POSITIONS+1] = {position:GetSongBeat(), position:GetCurBPS(),
                position:GetMusicSeconds(), position:GetFreeze(), position:GetDelay()}
        end
        GLOBAL_BEAT = GAMESTATE:GetSongPosition():GetSongBeat()
    end)
end }
"#, |context| {
            context.defer_initial_live_update = true;
            context.players[1].enabled = true;
            context.song_timing = Some(TimingData::from_segments(0.0, 0.0,
                &TimingSegments { bpms: vec![(0.0, 60.0)], ..Default::default() }, &[]));
            context.player_timing = [
                Some(TimingData::from_segments(0.0, 0.0,
                    &TimingSegments { bpms: vec![(0.0, 120.0)],
                        warps: vec![WarpSegment { beat: 4.0, length: 20.0 }],
                        ..Default::default() }, &[])),
                Some(TimingData::from_segments(0.0, 0.0,
                    &TimingSegments { bpms: vec![(0.0, 60.0)],
                        stops: vec![StopSegment { beat: 1.0, duration: 1.0 }],
                        delays: vec![DelaySegment { beat: 3.0, duration: 0.5 }], ..Default::default() }, &[])),
            ];
        });
        let mut scene = compiled.live_scene.take().expect("live scene");
        for seconds in [1.5_f32, 2.5, 4.25] {
            let output = scene.advance(SongLuaLiveFrame { music_seconds: seconds.into(),
                screen_seconds: seconds.into(), delta_seconds: 0.1,
                ..Default::default() }).expect("live frame");
            assert!(output.errors.is_empty());
            let positions = scene.lua.globals().get::<Table>("POSITIONS").unwrap();
            for player in 0..LUA_PLAYERS {
                let expected = scene.context.player_timing[player].as_ref().unwrap()
                    .get_song_position(seconds);
                let actual = positions.get::<Table>(player + 1).unwrap();
                assert_eq!(actual.get::<f32>(1).unwrap(), expected.beat, "player {player}");
                assert_eq!(actual.get::<f32>(2).unwrap(), expected.bpm / 60.0);
                assert_eq!(actual.get::<f32>(3).unwrap(), seconds);
                assert_eq!(actual.get::<bool>(4).unwrap(), expected.is_in_freeze);
                assert_eq!(actual.get::<bool>(5).unwrap(), expected.is_in_delay);
            }
            assert_eq!(scene.lua.globals().get::<f32>("GLOBAL_BEAT").unwrap(), seconds);
        }
    }

    #[test]
    fn live_stats() {
        let mut compiled = compile("stats", r#"
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local stage = STATSMAN:GetCurStageStats()
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        OBSERVED = {stats:GetActualDancePoints(), stats:GetPossibleDancePoints(),
            stats:GetCurrentPossibleDancePoints(), stats:GetCurrentLife(), stats:GetFailed(),
            stats:GetCurrentCombo(), stats:GetCurrentMissCombo(), stats:GetTapNoteScores('TapNoteScore_Miss'),
            stats:GetTapNoteScores('TapNoteScore_W1'), stats:GetTapNoteScores('TapNoteScore_HitMine'),
            stage:GetGameplaySeconds()}
    end)
end }
"#);
        let mut scene = compiled.live_scene.take().expect("scene");
        let mut player = deadsync_gameplay::init_player_runtime();
        player.earned_grade_points = -24;
        player.current_possible_points = 20;
        player.life = 0.0;
        player.fail_time = Some(2.0);
        player.combo = 3;
        player.miss_combo = 2;
        player.judgment_counts = [3, 0, 0, 0, 0, 6];
        player.scoring_counts = [0, 0, 0, 0, 0, 2];
        player.mines_hit = 1;
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 3.5, delta_seconds: 0.5,
            players: [SongLuaLivePlayer::from_player(&player, 25), SongLuaLivePlayer::default()],
            ..SongLuaLiveFrame::default() }).expect("actual stage snapshot");
        assert!(output.errors.is_empty());
        let observed = scene.lua.globals().get::<Table>("OBSERVED").expect("callback values");
        assert_eq!(observed.get::<i32>(1).expect("actual"), -24);
        assert_eq!(observed.get::<i32>(2).expect("maximum"), 25);
        assert_eq!(observed.get::<i32>(3).expect("progress"), 20);
        assert_eq!(observed.get::<f32>(4).expect("life"), 0.0);
        assert!(observed.get::<bool>(5).expect("failure"));
        for (index, expected) in [(6, 3), (7, 2), (8, 6), (9, 3), (10, 1)] {
            assert_eq!(observed.get::<u32>(index).expect("native count"), expected);
        }
        assert_eq!(observed.get::<f64>(11).expect("screen seconds"), 3.5);
    }

    #[test]
    fn live_reload() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/startup-reload-native.json")).expect("native control");
        let song = deadsync_simfile::song::parse_song_meta_file(&fixtures.join("startup-reload.ssc"),
            &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
            0.0, |_| 0.0).expect("native simfile");
        let mut context = SongLuaCompileContext::new(&fixtures, "Native reload");
        context.live = true;
        context.song_steps = song.charts.iter().map(crate::SongLuaStep::from).collect();
        context.player_step_indices[0] = context.song_steps.iter().position(|steps|
            steps.difficulty == SongLuaDifficulty::Challenge);
        context.players[0].difficulty = SongLuaDifficulty::Challenge;
        let mut compiled = crate::compile_song_lua_with_default_host(&fixtures.join("startup-reload.lua"),
            &context, SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("retained native control");
        assert_eq!(compiled.startup_reloads, 1);
        let scene = compiled.live_scene.take().expect("single VM");
        assert_eq!(scene.lua.globals().get::<u32>("HARNESS_NATIVE_RELOAD_COUNT").expect("same VM count"), 2);
        let stats = &scene.players[0];
        assert_eq!(stats.get::<Function>("GetPossibleDancePoints").expect("maximum getter")
            .call::<i32>(stats.clone()).expect("current maximum"), native["snapshots"][1]["possible"].as_i64().expect("native maximum") as i32);
        assert_eq!(scene.context().song_steps[scene.context().player_step_indices[0].expect("selected chart")]
            .difficulty, SongLuaDifficulty::Easy);
    }

    #[test]
    fn live_reload_resets_constructor_song_position() {
        let mut compiled = compile_with_context("reload-song-position", r#"
CLOCK_SCENE = (CLOCK_SCENE or 0) + 1
local scene = CLOCK_SCENE
local bps = GAMESTATE:GetSongBPS()
assert(bps == 10, 'constructors observe SongPosition::Reset, including on reload')
assert(GAMESTATE:GetSongPosition():GetCurBPS() == 10)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetSongPosition():GetCurBPS() == 10)
return Def.ActorFrame {
    InitCommand=function(self) CLOCK_ACTOR = self end,
    OnCommand=function(self)
        assert(math.abs(GAMESTATE:GetSongBPS() - 155/60) < 0.00001,
            'the first update replaces reset BPS with song timing')
        self:smooth(8/bps):rotationz(90)
        if scene == 1 then SCREENMAN:SetNewScreen('ScreenGameplay') end
    end,
}
"#, |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.song_timing_bpms = vec![(0.0, 155.0)];
        });
        let mut scene = compiled.live_scene.take().expect("first scene");
        let frame = SongLuaLiveFrame::default();
        let output = scene.advance(frame.clone()).expect("first song update");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(output.request, Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into())));
        let mut layers = crate::reload_song_lua_with_default_host(scene,
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("replacement constructors");
        let mut scene = layers[0].live_scene.take().expect("replacement scene");
        let output = scene.advance(frame).expect("replacement first update");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let actor: Table = scene.lua.globals().get("CLOCK_ACTOR").expect("replacement tween owner");
        let remaining = crate::lua_util::actor_tween_time_left(&scene.lua, &actor).unwrap();
        assert!((remaining - 0.8).abs() < 1e-6,
            "replacement tween uses its cached construction BPS: {remaining}");
    }

    #[test]
    fn live_reload_updates() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-reload-updates-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("reload directory");
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
            RELOAD_PASS = (RELOAD_PASS or 0) + 1
            local pass = RELOAD_PASS
            UPDATES = UPDATES or {0, 0}
            return Def.ActorFrame {
                InitCommand=function(self)
                    -- Installing a new callback invalidates the update plan.
                    -- Leave scene 2 without one so the test cannot hide a
                    -- stale plan by replacing it through SetUpdateFunction.
                    if pass == 1 then
                        self:SetUpdateFunction(function() UPDATES[pass] = UPDATES[pass] + 1 end)
                    end
                end,
                OnCommand=function(self)
                    self:sleep(2)
                    if pass == 1 then SCREENMAN:SetNewScreen('ScreenGameplay') end
                end,
                Def.ActorFrame {
                    OnCommand=function(self) self:sleep(0.05):queuecommand('Tick') end,
                    TickCommand=function(self)
                        TICK_PASS = pass
                        SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_DoNextScreen')
                    end,
                },
            }
        "#).expect("reload source");
        let mut context = SongLuaCompileContext::new(&dir, "reload updates");
        context.live = true;
        context.layer_start_beats = vec![0.0];
        let mut compiled = crate::compile_song_lua_with_default_host(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("first screen");
        let mut scene = compiled.live_scene.take().expect("first VM scene");
        let mut frame = SongLuaLiveFrame::default();
        frame.players[0].last_judgment = Some(deadsync_gameplay::JudgmentRenderInfo {
            started_at_screen_s: 0.0,
            judgment: deadsync_rules::judgment::Judgment {
                grade: deadsync_rules::judgment::JudgeGrade::Fantastic,
                window: Some(deadsync_rules::judgment::TimingWindow::W0),
                time_error_ms: 0.0, time_error_music_ns: 0, miss_because_held: false,
            },
        });
        let output = scene.advance(frame.clone()).expect("first foreground On");
        assert_eq!(output.request, Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into())));
        let mut layers = crate::reload_song_lua_with_default_host(scene,
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("replacement screen");
        let mut scene = layers[0].live_scene.take().expect("replacement scene");
        let player: Table = scene.lua.globals().get("__songlua_top_screen_player_1").expect("replacement player");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &player).expect("new player queue"),
            0.0, "a retired screen's judgment must not animate the replacement during startup");
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.1, beat: 0.1,
            screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).expect("replacement update");
        assert_eq!(output.request, Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into())));
        let updates = scene.lua.globals().get::<Table>("UPDATES").expect("scene updates");
        assert_eq!(updates.get::<u32>(1).expect("retired updates"), 1);
        assert_eq!(updates.get::<u32>(2).expect("replacement has no callback"), 0);
        assert_eq!(scene.lua.globals().get::<u32>("TICK_PASS").expect("new child queue"), 2);
    }

    #[test]
    fn live_screen_direct_and_queued_commands_reach_song_children() {
        let mut compiled = compile("screen-command-children", r#"
CALLS={}; PHASE='immediate'; MARKER={value=17}
local function probe(tag)
    return function(self, params)
        if PHASE=='immediate' then assert(params==MARKER) else assert(params==nil) end
        CALLS[#CALLS+1]=tag..':'..PHASE
    end
end
return Def.ActorFrame {
    ScreenCommandProbeCommand=probe('root'),
    OnCommand=function(self)
        self:sleep(1000)
        local screen=SCREENMAN:GetTopScreen()
        screen:playcommand('ScreenCommandProbe', MARKER)
        PHASE='queued'
        screen:sleep(0.25):queuecommand('ScreenCommandProbe')
    end,
    Def.ActorFrame { ScreenCommandProbeCommand=probe('nested'),
        Def.Actor { ScreenCommandProbeCommand=probe('leaf') },
    },
}
"#);
        let scene = compiled.live_scene.as_mut().expect("owned scene");
        let startup = scene.take_startup_output();
        assert!(startup.errors.is_empty(), "{:?}", startup.errors);
        let calls = || scene.lua.globals().get::<Table>("CALLS").unwrap()
            .sequence_values::<String>().collect::<mlua::Result<Vec<_>>>().unwrap();
        assert_eq!(calls(), ["root:immediate", "nested:immediate", "leaf:immediate"],
            "native Screen commands must reach nested song actors");
        let early = scene.advance(SongLuaLiveFrame { screen_seconds: 0.125,
            delta_seconds: 0.125, ..Default::default() }).expect("before queued command");
        assert!(early.errors.is_empty(), "{:?}", early.errors);
        assert_eq!(scene.lua.globals().get::<Table>("CALLS").unwrap().raw_len(), 3);
        let due = scene.advance(SongLuaLiveFrame { screen_seconds: 0.375,
            delta_seconds: 0.25, ..Default::default() }).expect("queued screen command");
        assert!(due.errors.is_empty(), "{:?}", due.errors);
        let calls = scene.lua.globals().get::<Table>("CALLS").unwrap()
            .sequence_values::<String>().collect::<mlua::Result<Vec<_>>>().unwrap();
        assert_eq!(calls, ["root:immediate", "nested:immediate", "leaf:immediate",
            "root:queued", "nested:queued", "leaf:queued"],
            "the screen queue advances independently of its sleeping descendants");
        scene.advance(SongLuaLiveFrame { screen_seconds: 0.5,
            delta_seconds: 0.125, ..Default::default() }).expect("later update");
        assert_eq!(scene.lua.globals().get::<Table>("CALLS").unwrap().raw_len(), 6,
            "each descendant receives each direct command once");
    }

    #[test]
    fn live_source_broadcasts() {
        let mut compiled = compile("broadcast-origin", r#"
            return Def.ActorFrame {
                OnCommand=function(self)
                    GAMESTATE:SetCurrentSteps(PLAYER_1, GAMESTATE:GetCurrentSteps(PLAYER_1))
                    MESSAGEMAN:Broadcast('CurrentStepsP1Changed')
                end,
                CurrentStepsP1ChangedMessageCommand=function(self)
                    MESSAGEMAN:Broadcast('NestedSource')
                end,
            }
        "#);
        let output = compiled.live_scene.as_mut().expect("owned scene").take_startup_output();
        assert_eq!(output.source_broadcasts.iter().map(|event| event.name.as_str()).collect::<Vec<_>>(),
            ["NestedSource", "CurrentStepsP1Changed", "NestedSource"]);
        assert_eq!(output.broadcasts.iter().filter(|(_, name, _)| name == "CurrentStepsP1Changed").count(), 2);
        assert_eq!(output.broadcasts.iter().filter(|(_, name, _)| name == "ScreenChanged").count(), 1);
    }

    #[test]
    fn live_queued_broadcast_origin() {
        let mut compiled = compile("queued-broadcast-origin", r#"
            return Def.ActorFrame {
                OnCommand=function(self) self:sleep(0.25):queuemessage('Queued') end,
                QueuedMessageCommand=function(self, params)
                    assert(type(params) == 'table' and next(params) == nil)
                    MESSAGEMAN:Broadcast('NestedSource', params)
                end,
            }
        "#);
        let scene = compiled.live_scene.as_mut().expect("owned scene");
        assert!(scene.take_startup_output().source_broadcasts.is_empty());
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 0.3125,
            delta_seconds: 0.3125, ..Default::default() }).expect("queued native message");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(output.broadcasts.iter().map(|(_, name, params)| (name.as_str(), *params)).collect::<Vec<_>>(),
            [("Queued", true), ("NestedSource", true)]);
        assert_eq!(output.source_broadcasts.iter().map(|event| event.name.as_str()).collect::<Vec<_>>(),
            ["NestedSource"]);
        assert_eq!(output.source_broadcasts[0].params.as_ref().expect("empty listener table"),
            &Some(serde_json::json!([])));
    }

    #[test]
    fn live_source_function_params() {
        let mut compiled = compile("broadcast-function-params", r#"local function transform(value)
    CALLS=(CALLS or 0)+1
    return value+2
end
ORIGINAL_TRANSFORM=transform
return Def.ActorFrame {
    OnCommand=function(self)
        CALLS=0
        MESSAGEMAN:Broadcast('FunctionSnapshot', {transform, nested={callback=transform}, native=math.abs})
    end,
    FunctionSnapshotMessageCommand=function(self, params)
        assert(params[1]==ORIGINAL_TRANSFORM and params.nested.callback==ORIGINAL_TRANSFORM)
        assert(params.native==math.abs)
        RESULT=params[1](3)+params.nested.callback(4)+params.native(-7)
    end,
}"#);
        let scene = compiled.live_scene.as_mut().expect("owned scene");
        let output = scene.take_startup_output();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(scene.lua.globals().get::<u32>("CALLS").expect("listener function calls"), 2);
        assert_eq!(scene.lua.globals().get::<u32>("RESULT").expect("listener result"), 18);
        let params = output.source_broadcasts[0].params.as_ref()
            .expect("function metadata must preserve callable broadcast parameters").as_ref().expect("params");
        let function = &params["1"];
        assert_eq!(function["type"], "function");
        assert!(function["source"].as_str().expect("Lua source").ends_with("/default.lua"));
        assert_eq!(function["line"], 1);
        assert_eq!(function["last_line"], 4);
        assert_eq!(&params["nested"]["callback"], function);
        assert_eq!(params["native"], serde_json::json!({
            "type":"function", "source":"=[C]", "line":-1, "last_line":-1
        }));
    }

    #[test]
    fn live_simply_love_sm_broadcasts() {
        let mut compiled = compile("simply-love-sm-broadcasts", r#"
return Def.Actor {
    OnCommand=function(self)
        RECEIVED={}; TRACED={}
        Trace=function(msg)
            assert(#RECEIVED==#TRACED+1, 'SM broadcasts before tracing')
            TRACED[#TRACED+1]=msg
        end
        SM('hello')
        SM(17, 0, false)
        SM(false, 2.5, true)
        SM(nil)
        local cycle={}; cycle[1]=cycle; SM(cycle)
        SM({'nested'})
    end,
    SystemMessageMessageCommand=function(self, params)
        RECEIVED[#RECEIVED+1]=params
        params.Message='listener-mutated'
    end,
}
"#);
        let scene = compiled.live_scene.as_mut().expect("owned scene");
        let output = scene.take_startup_output();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(output.source_broadcasts.len(), 6, "SM must dispatch Simply Love SystemMessage broadcasts");
        let expected = [
            serde_json::json!({"Message":"hello"}),
            serde_json::json!({"Message":"17", "Duration":0, "Stack":false}),
            serde_json::json!({"Message":"false", "Duration":2.5, "Stack":true}),
            serde_json::json!({"Message":"nil"}),
            serde_json::json!({"Message":"Value = {\n|    [1] = {} -- Value (self reference)\n}"}),
            serde_json::json!({"Message":"Value = {\n|    [1] = \"nested\"\n}"}),
        ];
        let received: Table = scene.lua.globals().get("RECEIVED").expect("listeners ran");
        let traced: Table = scene.lua.globals().get("TRACED").expect("trace calls");
        assert_eq!(received.raw_len(), 6);
        assert_eq!(traced.raw_len(), 6);
        for (index, (event, expected)) in output.source_broadcasts.iter().zip(expected).enumerate() {
            assert_eq!(event.name, "SystemMessage");
            assert_eq!(event.params.as_ref().expect("trace params"), &Some(expected.clone()));
            assert_eq!(traced.raw_get::<String>(index + 1).expect("original Trace value"),
                expected["Message"].as_str().unwrap());
            assert_eq!(received.raw_get::<Table>(index + 1).unwrap().raw_get::<String>("Message").unwrap(),
                "listener-mutated");
        }
        scene.lua.load(r#"
            assert(TableToString({}, 'Root', '>')=='>Root = {}')
            local original=TableToString
            TableToString=function(value) assert(value.flag); return 'custom formatter' end
            SM({flag=true}, 4, false)
            assert(TRACED[7]=='custom formatter')
            TableToString=original
        "#).exec().expect("SM uses the current public formatter");
    }

    #[test]
    fn live_sprite_cropto_alias() {
        let mut compiled = compile_with_context("sprite-cropto-alias", r#"
return Def.ActorFrame {
    Def.Sprite {
        Texture='wide.png',
        OnCommand=function(self)
            self:xy(17,29)
            assert(self:cropto(100,100)==self, 'cropto returns its receiver')
            assert(self:GetWidth()==100 and self:GetHeight()==100 and self:GetZoomedWidth()==100)
            assert(self:GetX()==17 and self:GetY()==29)
            local original=Sprite.CropTo
            local marker={}; local calls=0
            Sprite.CropTo=function(receiver,w,h)
                assert(receiver==self and w==marker and h==false, 'cropto forwards original arguments')
                calls=calls+1
            end
            assert(self:cropto(marker,false)==self and calls==1, 'cropto uses current CropTo method')
            Sprite.CropTo=original
            MESSAGEMAN:Broadcast('CropAliasVerified', {kind='wide', width=self:GetWidth(), height=self:GetHeight()})
        end,
    },
    Def.Sprite {
        Texture='tall.png',
        OnCommand=function(self)
            assert(self:cropto(100,100)==self)
            assert(self:GetWidth()==100 and self:GetHeight()==100 and self:GetZoomedHeight()==100)
            MESSAGEMAN:Broadcast('CropAliasVerified', {kind='tall', width=self:GetWidth(), height=self:GetHeight()})
        end,
    },
}
"#, |context| {
            image::RgbaImage::new(200,100).save(context.song_dir.join("wide.png")).unwrap();
            image::RgbaImage::new(100,200).save(context.song_dir.join("tall.png")).unwrap();
        });
        let output = compiled.live_scene.as_mut().expect("owned scene").take_startup_output();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(output.source_broadcasts.len(), 2);
        for (event, kind) in output.source_broadcasts.iter().zip(["wide", "tall"]) {
            assert_eq!(event.name, "CropAliasVerified");
            assert_eq!(event.params.as_ref().unwrap(), &Some(serde_json::json!({"kind":kind,"width":100,"height":100})));
        }
    }

    #[test]
    fn live_source_params() {
        let mut compiled = compile("broadcast-params", r#"
            return Def.ActorFrame {
                OnCommand=function(self)
                    LOOKUPS=0
                    local params = setmetatable({ count=2, nested={true, 'original'}, [3]='mixed' }, {
                        __index=function() LOOKUPS=LOOKUPS+1 end,
                    })
                    MESSAGEMAN:Broadcast('Snapshot', params)
                    local cycle = {}; cycle.self = cycle
                    MESSAGEMAN:Broadcast('Unsupported', cycle)
                    MESSAGEMAN:Broadcast('Empty', {})
                    MESSAGEMAN:Broadcast('NoParams')
                end,
                SnapshotMessageCommand=function(self, params)
                    params.count=99; params.nested[2]='mutated'
                end,
                UnsupportedMessageCommand=function(self) DELIVERED=true end,
            }
        "#);
        let scene = compiled.live_scene.as_mut().expect("owned scene");
        let output = scene.take_startup_output();
        assert!(output.errors.is_empty(), "trace capture must not interrupt Lua");
        assert_eq!(scene.lua.globals().get::<u32>("LOOKUPS").expect("raw trace reads"), 0);
        assert_eq!(output.source_broadcasts[0].params.as_ref().expect("plain values"),
            &Some(serde_json::json!({"count":2,"nested":[true,"original"],"3":"mixed"})));
        assert!(output.source_broadcasts[1].params.as_ref().expect_err("cycle stays pending").contains("cycle"));
        assert!(scene.lua.globals().get::<bool>("DELIVERED").expect("listener ran"));
        assert_eq!(output.source_broadcasts[2].params.as_ref().expect("empty table"),
            &Some(serde_json::json!([])));
        assert_eq!(output.source_broadcasts[3].params.as_ref().expect("no params"), &None);
    }

    #[test]
    fn live_end_output() {
        let mut compiled = compile("end-output", r#"
            SCENES=(SCENES or 0)+1
            local count=SCENES
            return Def.ActorFrame {
                OnCommand=function(self) MESSAGEMAN:Broadcast('Entry', {count=count}) end,
                EndCommand=function(self)
                    MESSAGEMAN:Broadcast('RootEnd', {count=count, top=SCREENMAN:GetTopScreen() ~= nil})
                end,
                Def.Actor {
                    EndCommand=function(self)
                        MESSAGEMAN:Broadcast('ChildEnd', {count=count})
                        self:queuecommand('Retired')
                    end,
                    RetiredCommand=function(self) MESSAGEMAN:Broadcast('Retired') end,
                },
            }
        "#);
        let mut scene = compiled.live_scene.take().expect("first scene");
        scene.take_startup_output();
        scene.advance(SongLuaLiveFrame { music_seconds: 2.0, beat: 4.0,
            screen_seconds: 10.0, delta_seconds: 0.02, ..Default::default() }).expect("old frame");
        let mut compiled = crate::reload_song_lua_with_default_host(scene,
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("same-VM reload");
        let mut scene = compiled[0].live_scene.take().expect("replacement");
        let output = scene.take_startup_output();
        assert_eq!(output.source_broadcasts.iter().map(|event| event.name.as_str()).collect::<Vec<_>>(),
            ["RootEnd", "ChildEnd", "Entry"]);
        assert_eq!(output.source_broadcasts[0].music_seconds, 2.0);
        assert_eq!(output.source_broadcasts[1].music_seconds, 2.0);
        assert_eq!(output.source_broadcasts[2].music_seconds, 0.0);
        assert_eq!(output.source_broadcasts[0].params.as_ref().expect("plain end params"),
            &Some(serde_json::json!({"count":1,"top":false})));
        let output = scene.end().expect("final end");
        assert_eq!(output.source_broadcasts.len(), 2);
        assert!(scene.end().expect("end only once").source_broadcasts.is_empty());
        assert!(scene.advance(SongLuaLiveFrame::default()).is_err(), "ended actors must not update");
    }

    #[test]
    fn live_scheduled_reload() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro");
        let song = deadsync_simfile::song::parse_song_meta_file(&fixtures.join("startup-reload.ssc"),
            &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
            0.0, |_| 0.0).expect("native reload simfile");
        let mut context = SongLuaCompileContext::new(&fixtures, "Scheduled native reload");
        context.live = true;
        context.layer_start_beats = vec![0.0];
        context.song_steps = song.charts.iter().map(crate::SongLuaStep::from).collect();
        context.player_step_indices[0] = context.song_steps.iter().position(|steps|
            steps.difficulty == SongLuaDifficulty::Challenge);
        context.players[0].difficulty = SongLuaDifficulty::Challenge;
        let mut compiled = crate::compile_song_lua_with_default_host(&fixtures.join("startup-reload.lua"),
            &context, SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("scheduled scene");
        assert_eq!(compiled.startup_reloads, 0);
        let mut scene = compiled.live_scene.take().expect("first scene");
        let selected = scene.selected_context().expect("raw GAMESTATE selection");
        assert_ne!(context.player_step_indices[0], selected.player_step_indices[0]);
        assert_eq!(scene.context().player_step_indices, context.player_step_indices);
        scene.lua.load(r#"
            math.randomseed(73)
            math.random()
            EXPECTED_RANDOM = math.random()
            math.randomseed(73)
            math.random()
            OLD_MESSAGES, OLD_QUEUES, ENDED = 0, 0, 0
            local ps = GAMESTATE:GetPlayerState(PLAYER_1)
            RETAINED_PREFERRED = ps:GetPlayerOptions('ModsLevel_Preferred')
            RETAINED_CURRENT = ps:GetPlayerOptions('ModsLevel_Current')
            RETAINED_PREFERRED:Dark(0.25)
            RETAINED_CURRENT:Dark(0.5)
            ps:GetPlayerOptions('ModsLevel_Song'):Dark(1, 0.1)
        "#).exec().expect("random and lifetime controls");
        let old_actor = scene.actors[0].clone();
        old_actor.set("RetiredMessageCommand", scene.lua.create_function(|lua, _: Table| {
            let globals = lua.globals();
            globals.set("OLD_MESSAGES", globals.get::<u32>("OLD_MESSAGES")? + 1)
        }).expect("old message listener")).expect("install old listener");
        old_actor.set("EndCommand", scene.lua.create_function(|lua, _: Table| {
            let globals = lua.globals();
            globals.set("ENDED", globals.get::<u32>("ENDED")? + 1)
        }).expect("old End command")).expect("install End");
        old_actor.set("StaleCommand", scene.lua.create_function(|lua, _: Table| {
            lua.globals().set("OLD_QUEUES", 1)
        }).expect("old queued command")).expect("install stale command");
        old_actor.get::<Function>("sleep").expect("sleep method")
            .call::<Table>((old_actor.clone(), 10.0)).expect("old sleep");
        old_actor.get::<Function>("queuecommand").expect("queue method")
            .call::<Table>((old_actor.clone(), "Stale")).expect("old queue");
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.017, beat: 0.0,
            screen_seconds: 2.0, delta_seconds: 0.001, ..Default::default() }).expect("activate foreground");
        assert_eq!(output.request, Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into())));
        let mut layers = crate::reload_song_lua_with_default_host(scene,
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("runtime reload");
        let mut scene = layers[0].live_scene.take().expect("replacement scene");
        assert_eq!(scene.context().player_step_indices, selected.player_step_indices);
        assert_eq!(scene.lua.globals().get::<u32>("HARNESS_NATIVE_RELOAD_COUNT").expect("same VM"), 2);
        assert_eq!(scene.lua.globals().get::<u32>("ENDED").expect("old End delivered"), 1);
        let dark = scene.lua.load("return RETAINED_PREFERRED:Dark(), RETAINED_CURRENT:Dark(), GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song'):Dark()")
            .eval::<(f32, f32, f32)>().expect("restarted regular stage options");
        assert_eq!(dark, (0.25, 0.25, 0.25));
        assert_eq!(scene.lua.load("return math.random()").eval::<f64>().expect("next random"),
            scene.lua.globals().get::<f64>("EXPECTED_RANDOM").expect("retained generator"));
        assert_eq!(scene.players[0].get::<Function>("GetPossibleDancePoints").expect("possible getter")
            .call::<i32>(scene.players[0].clone()).expect("replacement maximum"), 10);
        scene.lua.load("MESSAGEMAN:Broadcast('Retired')").exec().expect("new registry broadcast");
        scene.advance(SongLuaLiveFrame { music_seconds: 10.0, beat: 20.0,
            screen_seconds: 10.0, delta_seconds: 10.0, ..Default::default() }).expect("new scene update");
        assert_eq!(scene.lua.globals().get::<u32>("OLD_MESSAGES").expect("retired listener"), 0);
        assert_eq!(scene.lua.globals().get::<u32>("OLD_QUEUES").expect("retired queue"), 0);
    }

    #[test]
    fn live_failed_actor_load_keeps_vm_and_rebuilds_on_reload() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-load-fallback-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("song directory");
        let background = dir.join("background.lua");
        let foreground = dir.join("foreground.lua");
        let invalid = dir.join("invalid.lua");
        fs::write(&background, r#"
BACKGROUND_LOADS = (BACKGROUND_LOADS or 0) + 1
if COUNTER == nil then error('native failed actor load uses a dummy') end
return Def.ActorFrame{InitCommand=function(self) BACKGROUND_INITS = COUNTER end}
"#).expect("background source");
        fs::write(&foreground, r#"
COUNTER = COUNTER or 0
return Def.ActorFrame{OnCommand=function(self)
    if not RELOADED then RELOADED = true; SCREENMAN:SetNewScreen('ScreenGameplay') end
end}
"#).expect("foreground source");
        fs::write(&invalid, "INVALID_LOADS = (INVALID_LOADS or 0) + 1; return 42").expect("invalid return source");
        let mut context = SongLuaCompileContext::new(&dir, "native actor load fallback");
        context.live = true;
        context.background_layer_count = 1;
        context.layer_start_beats = vec![4.0, 0.0, 0.0];
        context.foreground_names = vec!["foreground".into(), "invalid".into()];
        let mut layers: Vec<Compiled> = crate::compile_song_lua_layers_with_default_host(
            &[&background, &foreground, &invalid], 1, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None)
            .expect("native failed actor load uses a dummy and preserves the shared VM");
        let mut scene = layers[1].live_scene.take().expect("primary retains shared scene");
        assert_eq!(scene.lua.globals().get::<u32>("BACKGROUND_LOADS").unwrap(), 1);
        assert_eq!(scene.lua.globals().get::<u32>("INVALID_LOADS").unwrap(), 1);
        assert!(scene.lua.globals().get::<Option<u32>>("BACKGROUND_INITS").unwrap().is_none());
        scene.lua.load(r#"
local children = SCREENMAN:GetTopScreen():GetChild('SongForeground'):GetChildren()
assert(children.foreground ~= children.invalid)
assert(children.invalid.__songlua_actor_type == 'Actor')
"#).exec().expect("native dummy occupies its foreground slot");
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.1, beat: 0.1,
            screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).expect("activate foreground");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(output.request, Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into())));
        let mut layers: Vec<Compiled> = crate::reload_song_lua_with_default_host(scene,
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("reload same process VM");
        let mut scene = layers[1].live_scene.take().expect("replacement retains scene");
        assert_eq!(scene.lua.globals().get::<u32>("BACKGROUND_LOADS").unwrap(), 2);
        assert_eq!(scene.lua.globals().get::<u32>("BACKGROUND_INITS").unwrap(), 0);
        assert_eq!(scene.lua.globals().get::<u32>("INVALID_LOADS").unwrap(), 2);
        assert!(scene.advance(SongLuaLiveFrame { music_seconds: 1.0, beat: 4.0,
            screen_seconds: 1.0, delta_seconds: 0.1, ..Default::default() }).unwrap().errors.is_empty());
    }

    #[test]
    fn live_loadfile_preserves_secondary_ready_function_and_nil_results() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-loader-results-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("song directory");
        fs::write(dir.join("module.lua"), r#"
local value = ...
return function() UPDATES = (UPDATES or 0) + value end,
    function() READY = (READY or 0) + 1 end, nil, 17, nil
"#).expect("multiple return module");
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
local chunk = assert(loadfile('module.lua'))
local values = {n=select('#',chunk(3)),chunk(3)}
RESULT_COUNT = values.n
assert(values.n == 5, 'scoped loadfile must retain all return values')
assert(type(values[2]) == 'function' and values[3] == nil and values[4] == 17 and values[5] == nil)
local update, ready = values[1], values[2]
return Def.ActorFrame{OnCommand=function(self)
    ready()
    self:SetUpdateFunction(function(self, delta) if delta > 0 then update() end end)
end}
"#).expect("ready callback source");
        let mut compiled = compile_with_context("loader-results", &fs::read_to_string(&entry).unwrap(), |context| {
            context.song_dir = dir.clone();
        });
        let scene = compiled.live_scene.as_mut().expect("retained live scene");
        assert_eq!(scene.lua.globals().get::<u32>("RESULT_COUNT").unwrap(), 5,
            "scoped loadfile must retain all return values");
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.1, beat: 0.1,
            screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(scene.lua.globals().get::<u32>("READY").expect("secondary ready function ran"), 1);
        assert_eq!(scene.lua.globals().get::<u32>("UPDATES").expect("first function retained its argument"), 3);
    }

    #[test]
    fn live_sparse_declarations_keep_native_child_callback_order() {
        let mut compiled = compile("sparse-declaration-order", r#"
local function native_def(class, t)
    t.Class = class
    t._Source = '@sparse-declaration-order.lua'
    t._Line = 1
    t._Dir = './'
    return setmetatable(t, {})
end
local function root(ctor)
    local a = ctor('ActorFrame', {OnCommand=function() end,
        ctor('Actor', {Name='yolo', InitCommand=function(self) self:sleep(420) end}),
        ctor('Sprite', {Name='leader', OnCommand=function(self) table.insert(ON_ORDER, self:GetName()) end})})
    for offset, name in ipairs({'border','badge','left','right','black','sound'}) do
        a[#a + offset] = ctor(offset == 6 and 'Actor' or 'Sprite', {
            Name=name, OnCommand=function(self) table.insert(ON_ORDER, self:GetName()) end})
    end
    return a
end
local native = root(native_def)
local expected, prefix = {}, 0
for _, child in ipairs(native) do
    if type(child) == 'table' then
        if type(child.OnCommand) == 'function' then expected[#expected+1] = child.Name end
        prefix = prefix + 1
    end
end
for key, child in pairs(native) do
    if type(child) == 'table' and type(child.OnCommand) == 'function' and type(key) == 'number' and not (key >= 1 and key <= prefix and key % 1 == 0) then
        expected[#expected+1] = child.Name
    end
end
NATIVE_ORDER = table.concat(expected, ',')
ON_ORDER = {}
return root(function(class, t) return Def[class](t) end)
"#);
        let scene = compiled.live_scene.as_mut().expect("retained scene");
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.1, beat: 0.1,
            screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let actual = scene.lua.load("return table.concat(ON_ORDER, ',')").eval::<String>().unwrap();
        let expected = scene.lua.globals().get::<String>("NATIVE_ORDER").unwrap();
        assert_eq!(actual, expected, "native sparse declarations retain child callback order");
    }

    #[test]
    fn live_layers() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-layers-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("layer directory");
        let background = dir.join("background.lua");
        let foreground = dir.join("foreground.lua");
        fs::write(&background, r#"
LIVE_ORDER = 0
return Def.ActorFrame { Name = 'Background', OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta > 0 then LIVE_ORDER = LIVE_ORDER + 1; self:x(LIVE_ORDER) end
    end)
end }
"#).expect("background script");
        fs::write(&foreground, r#"
return Def.ActorFrame { Name = 'Foreground', OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta > 0 then self:x(2 * LIVE_ORDER) end
    end)
end }
"#).expect("foreground script");
        let mut context = SongLuaCompileContext::new(&dir, "shared layers");
        context.live = true;
        context.background_layer_count = 1;
        let mut layers = crate::compile_song_lua_layers_with_default_host(&[&background, &foreground],
            1, &context, SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("one shared VM");
        assert!(layers[0].live_scene.is_none());
        let mut scene = layers[1].live_scene.take().expect("primary owns the VM");
        let frame = SongLuaLiveFrame { screen_seconds: 0.1, delta_seconds: 0.1, ..SongLuaLiveFrame::default() };
        assert!(scene.advance(frame.clone()).expect("shared update").errors.is_empty());
        for (layer, name, x) in [(0, "Background", 1.0), (1, "Foreground", 2.0)] {
            let local = layers[layer].overlays.iter().position(|actor|
                actor.name.as_deref() == Some(name)).expect("layer actor");
            assert_eq!(scene.states()[layers[layer].live_actor_indices()[local]].x, x);
        }
    }

    #[test]
    fn live_null_foreground_keeps_later_entries_and_owns_each_instance() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-null-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("foreground directory");
        let blank = dir.join("blank.lua");
        let active = dir.join("active.lua");
        fs::write(&blank, "assert(NullActor.Class == 'Actor'); return NullActor").expect("empty declaration");
        fs::write(&active, r#"
ACTIVATIONS = 0
return Def.Actor { OnCommand = function(self)
    ACTIVATIONS = ACTIVATIONS + 1
    assert(GAMESTATE:IsPlayerEnabled(0))
    local children = self:GetParent():GetChildren()
    assert(children.first ~= children.last)
    children.first:aux(17)
    assert(children.last:getaux() == 0)
    assert(NullActor:getaux() == 0)
end }
"#).expect("later foreground");
        let mut context = SongLuaCompileContext::new(&dir, "null foreground");
        context.live = true;
        context.layer_start_beats = vec![0.0; 3];
        context.foreground_names = vec!["first".into(), "active".into(), "last".into()];
        let mut layers: Vec<Compiled> = crate::compile_song_lua_layers_with_default_host(
            &[&blank, &active, &blank], 0, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("all foreground entries");
        let mut scene = layers[0].live_scene.take().expect("primary retains scene");
        let output = scene.advance(SongLuaLiveFrame { music_seconds: 0.02, beat: 0.01,
            screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).expect("foreground activation");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(scene.lua.globals().get::<u32>("ACTIVATIONS").expect("later On"), 1);
    }

    #[test]
    fn live_screen_positions_survive_registration_and_seed_authored_tweens() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-screen-pose-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("screen position directory");
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
PLAYER = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
FIELD = PLAYER:GetChild('NoteField')
START_X, START_Y, START_FIELD_Y = PLAYER:GetX(), PLAYER:GetY(), FIELD:GetY()
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta > 0 and FIRST_X == nil then
            FIRST_X, FIRST_Y, FIRST_FIELD_Y = PLAYER:GetX(), PLAYER:GetY(), FIELD:GetY()
        end
    end)
end }
"#).expect("independent position control");
        let mut context = SongLuaCompileContext::new(&dir, "screen positions");
        context.live = true;
        context.players[0].screen_x = 231.25;
        context.players[0].screen_y = 199.0;
        context.players[1].screen_x = 620.0;
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None, |_, _| None).expect("configured screen positions");
        let mut scene = compiled.live_scene.take().expect("retained screen scene");
        let globals = scene.lua.globals();
        let field_y = f32::midpoint(crate::THEME_RECEPTOR_Y_STD, crate::THEME_RECEPTOR_Y_REV);
        for (name, value) in [("START_X", 231.25), ("START_Y", 199.0), ("START_FIELD_Y", field_y)] {
            assert_eq!(globals.get::<f32>(name).expect("startup getter"), value, "{name}");
        }
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 0.1,
            delta_seconds: 0.1, ..Default::default() }).expect("first live position frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        for (name, value) in [("FIRST_X", 231.25), ("FIRST_Y", 199.0), ("FIRST_FIELD_Y", field_y)] {
            assert_eq!(globals.get::<f32>(name).expect("positive frame getter"), value, "{name}");
        }
        scene.lua.load("PLAYER:linear(0.4):x(431.25)").exec().expect("authored player tween");
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 0.2,
            delta_seconds: 0.1, ..Default::default() }).expect("player tween advances");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene.lua.load("assert(math.abs(PLAYER:GetX() - 281.25) < 0.0001); assert(PLAYER:GetDestX() == 431.25)")
            .exec().expect("tween starts from the initialized pose");
    }

    #[test]
    fn live_native_mini_zoom_updates_before_song_callbacks_and_preserves_oitg_z() {
        let mut compiled = compile_with_context("native-mini-zoom", r#"
return Def.ActorFrame {InitCommand=function(self)
  PLAYER=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
  FIELD=PLAYER:GetChild('NoteField')
  assert(PLAYER:get_oitg_zoom_mode()==false)
  assert(PLAYER:set_oitg_zoom_mode(true)==PLAYER)
  FIELD:zoomz(3)
  local ps=GAMESTATE:GetPlayerState(PLAYER_1)
  ps:GetCurrentPlayerOptions():Mini(0)
  ps:GetPlayerOptions('ModsLevel_Song'):Mini(1,2)
  self:SetUpdateFunction(function()
    SEEN={FIELD:GetZoomX(),FIELD:GetZoomY(),FIELD:GetZoomZ()}
  end)
end}
"#, |context| context.defer_initial_live_update=true);
        let mut scene=compiled.live_scene.take().unwrap();
        let frame=|seconds| SongLuaLiveFrame {screen_seconds:seconds,delta_seconds:0.125,..Default::default()};
        assert!(scene.advance(frame(0.125)).unwrap().errors.is_empty());
        scene.lua.load("assert(SEEN[1]==0.875 and SEEN[2]==0.875 and SEEN[3]==3)").exec().unwrap();
        scene.lua.load("PLAYER:set_oitg_zoom_mode(false)").exec().unwrap();
        assert!(scene.advance(frame(0.25)).unwrap().errors.is_empty());
        scene.lua.load("assert(SEEN[1]==0.75 and SEEN[2]==0.75 and SEEN[3]==0.75)").exec().unwrap();
    }

    #[test]
    #[cfg(feature = "test-support")]
    fn live_native_option_clocks_are_independent_and_consumed_once() {
        let mut compiled=compile_with_context("independent-option-clocks","return Def.ActorFrame {}",
            |context| {context.defer_initial_live_update=true;context.players[1].enabled=true;});
        let mut scene=compiled.live_scene.take().unwrap();
        scene.lua.load(r#"
for _,pn in ipairs({PLAYER_1,PLAYER_2}) do
 local ps=GAMESTATE:GetPlayerState(pn)
 ps:GetCurrentPlayerOptions():Drunk(0)
 ps:GetPlayerOptions('ModsLevel_Song'):Drunk(1,1)
end
GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate(1)
GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(2,1)
"#).exec().unwrap();
        scene.set_option_clock_deltas([0.03125,0.0625,0.125]);
        let frame=|seconds| SongLuaLiveFrame {screen_seconds:seconds,delta_seconds:0.25,..Default::default()};
        assert!(scene.advance(frame(0.25)).unwrap().errors.is_empty());
        scene.lua.load(r#"
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate()==1.03125)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions():Drunk()==0.0625)
assert(GAMESTATE:GetPlayerState(PLAYER_2):GetCurrentPlayerOptions():Drunk()==0.125)
"#).exec().unwrap();
        assert!(scene.advance(frame(0.5)).unwrap().errors.is_empty());
        scene.lua.load(r#"
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate()==1.28125)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions():Drunk()==0.3125)
assert(GAMESTATE:GetPlayerState(PLAYER_2):GetCurrentPlayerOptions():Drunk()==0.375)
"#).exec().unwrap();
    }

    #[test]
    fn live_native_sound_pitch_preserves_identity_and_parameters() {
        let mut compiled = compile("native-sound-pitch", r#"
local primary = Def.Sound { InitCommand = function(self)
    PRIMARY = self:get()
    assert(PRIMARY:pitch('1.25'):volume(0.5) == PRIMARY)
    assert(not pcall(PRIMARY.pitch, PRIMARY))
end }
local other = Def.Sound { InitCommand = function(self) OTHER = self:get() end }
return Def.ActorFrame { primary, other,
    OnCommand = function(self) self:SetUpdateFunction(function(self, delta)
        if delta > 0 then PRIMARY:pitch(-0.5); self:aux(23) end
    end) end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("sound scene");
        let primary = scene.lua.globals().get::<Table>("PRIMARY").unwrap();
        let other = scene.lua.globals().get::<Table>("OTHER").unwrap();
        assert_eq!(primary.raw_get::<f32>("__songlua_pitch").unwrap(), 1.25);
        assert_eq!(primary.raw_get::<f32>("__songlua_volume").unwrap(), 0.5);
        assert_eq!(other.raw_get::<f32>("__songlua_pitch").unwrap(), 1.0);
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 0.1,
            delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(primary.raw_get::<f32>("__songlua_pitch").unwrap(), -0.5);
        assert_eq!(other.raw_get::<f32>("__songlua_pitch").unwrap(), 1.0);
        assert!(scene.states().iter().any(|state| state.aux == 23.0));
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn live_native_raw_clocks_apply_to_constructors_and_same_frame_reads() {
        let mut compiled = compile_with_context("raw-host-clock", "local started=GetTimeSinceStart()\nreturn Def.Quad{OnCommand=function(self)\n self:x(GetTimeSinceStart()-started):y(Minute()*60+Second())\nend}", |context| {
            context.host_clock_inputs = [
                ("GetTimeSinceStart", 1, 200.25),
                ("GetTimeSinceStart", 3, 200.75),
                ("Minute", 3, 17.0),
                ("Second", 3, 33.0),
            ].into_iter().map(|(clock,line,value)| crate::SongLuaHostClockInput {
                clock: clock.into(), source: "song:/default.lua".into(), line, value,
            }).collect();
        });
        let mut scene=compiled.live_scene.take().unwrap();
        let output=scene.advance(SongLuaLiveFrame::default()).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!(scene.states().iter().any(|state| state.x==0.5 && state.y==1053.0),
            "constructor uptime and each raw clock read must be independent of the frame snapshot");
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn live_native_arrow_clocks_keep_query_and_cache_update_reads_independent() {
        let mut compiled = compile_with_context("raw-arrow-clock", "return Def.ActorFrame{OnCommand=function(self)\n self:SetUpdateFunction(function(self)\n local ps=GAMESTATE:GetPlayerState(PLAYER_1)\n local po=ps:GetPlayerOptions('ModsLevel_Current');po:Drunk(1);po:Tipsy(1)\n self:x(ArrowEffects.GetXPos(ps,1,0)):y(ArrowEffects.GetXPos(ps,1,0))\n end)\nend}", |context| {
            context.defer_initial_live_update = true;
            context.host_clock_inputs = [50.0,51.0].into_iter().map(|value| crate::SongLuaHostClockInput {
                clock: "ArrowEffects.GetXPos".into(), source: "song:/default.lua".into(), line: 5, value,
            }).collect();
        });
        let mut scene = compiled.live_scene.take().unwrap();
        scene.set_arrow_clock_inputs([Some(40.0), None], [None, None]);
        let output = scene.advance(SongLuaLiveFrame { uptime_seconds: Some(1000.0), ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let expected = |time: f32| -96.0 + time.cos()*32.0;
        assert!(scene.states().iter().any(|state| (state.x-expected(50.0)).abs()<1e-5 && (state.y-expected(51.0)).abs()<1e-5));
        let y: f32 = scene.lua.load("return ArrowEffects.GetYPos(GAMESTATE:GetPlayerState(PLAYER_1),1,0,0)").eval().unwrap();
        assert!((y-(40.0_f32*1.2).cos()*25.6).abs()<1e-5, "cached Tipsy uses its own raw update clock: {y}");
    }

    #[test]
    fn live_native_broadcast_snapshot_preserves_explicit_nil_argument() {
        let mut compiled = compile("native-broadcast-arity", r#"
return Def.Actor { OnCommand = function()
    MESSAGEMAN:Broadcast('NoArgument')
    MESSAGEMAN:Broadcast('ExplicitNil', nil)
    MESSAGEMAN:Broadcast('TableArgument', {value=7})
end }
"#);
        let output = compiled.live_scene.as_mut().unwrap().take_startup_output();
        let events = output.source_broadcasts;
        assert_eq!(events.iter().map(|event| event.name.as_str()).collect::<Vec<_>>(),
            ["NoArgument", "ExplicitNil", "TableArgument"]);
        assert_eq!(events[0].params.as_ref().unwrap(), &None);
        assert_eq!(events[1].params.as_ref().unwrap(), &Some(serde_json::json!({"type":"nil"})));
        assert_eq!(events[2].params.as_ref().unwrap(), &Some(serde_json::json!({"value":7})));
    }

    #[test]
    fn live_sound_volume_keeps_the_sound_identity_and_owner() {
        let mut compiled = compile("sound-volume", r#"
local primary = Def.Sound { InitCommand = function(self)
    PRIMARY_SOUND = self:get()
    assert(PRIMARY_SOUND == self:get())
    assert(PRIMARY_SOUND:volume('1.5') == PRIMARY_SOUND)
    assert(not pcall(PRIMARY_SOUND.volume, PRIMARY_SOUND, {}))
end }
local other = Def.Sound { InitCommand = function(self) OTHER_SOUND = self:get() end }
return Def.ActorFrame { primary, other,
    OnCommand = function(self)
        self:SetUpdateFunction(function(self, delta)
            if delta > 0 then
                assert(PRIMARY_SOUND:volume(0.25) == PRIMARY_SOUND)
                self:aux(19)
            end
        end)
    end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("retained sound scene");
        let primary: Table = scene.lua.globals().get("PRIMARY_SOUND").expect("primary sound");
        let other: Table = scene.lua.globals().get("OTHER_SOUND").expect("other sound");
        assert_ne!(primary.to_pointer(), other.to_pointer());
        assert_eq!(primary.raw_get::<f32>("__songlua_volume").expect("unclamped native parameter"), 1.5);
        assert_eq!(other.raw_get::<f32>("__songlua_volume").expect("default sound parameter"), 1.0);
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 0.1,
            delta_seconds: 0.1, ..Default::default() }).expect("volume callback");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(primary.raw_get::<f32>("__songlua_volume").expect("updated sound parameter"), 0.25);
        assert_eq!(other.raw_get::<f32>("__songlua_volume").expect("independent other sound"), 1.0);
        assert!(scene.states().iter().any(|state| state.aux == 19.0), "callback continues after volume");
    }

    #[test]
    fn live_false_condition_skips_subtree_and_compacts_subscriber_paths() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-condition-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("condition directory");
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
local function excluded() error('excluded actor must never run') end
local function received(self) self:aux(self:getaux() + 1) end
return Def.ActorFrame {
    InitCommand = function(self)
        assert(self:GetNumChildren() == 2)
        local children = self:GetChildren()
        assert(children.Excluded == nil)
        assert(children.Group:GetNumChildren() == 1)
    end,
    OnCommand = function(self)
        MESSAGEMAN:Broadcast('Probe')
        assert(self:GetChild('First'):getaux() == 1)
        assert(self:GetChild('Group'):GetChild('Last'):getaux() == 1)
    end,
    Def.Quad { Name = 'Excluded', Condition = false, InitCommand = excluded,
        OnCommand = excluded, ProbeMessageCommand = excluded },
    Def.Actor { Name = 'First', Condition = 0, ProbeMessageCommand = received },
    Def.ActorFrame { Condition = false, InitCommand = excluded,
        Def.Quad { ProbeMessageCommand = excluded, OnCommand = excluded } },
    Def.ActorFrame { Name = 'Group',
        Def.Quad { Condition = false, InitCommand = excluded, ProbeMessageCommand = excluded },
        Def.Actor { Name = 'Last', Condition = true, ProbeMessageCommand = received } },
}
"#).expect("conditional actor tree");
        let mut context = SongLuaCompileContext::new(&dir, "conditional children");
        context.live = true;
        #[cfg(feature = "test-support")]
        {
            context.message_actor_order = [("1", 10), ("1/1", 20), ("1/2", 30), ("1/2/1", 40)]
                .into_iter().map(|(path, rank)| (path.into(), rank)).collect();
        }
        let mut compiled: Compiled = crate::compile_song_lua_with_default_host(
            &entry, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None)
            .expect("skip excluded initialization and broadcast subscriptions");
        let mut scene = compiled.live_scene.take().expect("retained active scene");
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 0.1,
            delta_seconds: 0.1, ..Default::default() }).expect("active conditional tree");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!(!compiled.overlays.iter().any(|actor| actor.name.as_deref() == Some("Excluded")));
    }

    #[test]
    fn live_requests() {
        let mut compiled = compile("transition", r#"
return Def.ActorFrame {
    OffCommand = function(self) self:aux(17) end,
    OnCommand = function(self)
        self:SetUpdateFunction(function(self, delta)
            if delta > 0 then
                SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_DoNextScreen')
                assert(self:getaux() == 17, 'OffCommand must run immediately')
                error('live diagnostic')
            end
        end)
    end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("scene");
        let frame = SongLuaLiveFrame { screen_seconds: 0.1, delta_seconds: 0.1, ..SongLuaLiveFrame::default() };
        let output = scene.advance(frame.clone()).expect("native partial callback");
        assert_eq!(output.request, Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into())));
        assert_eq!(output.errors.len(), 1);
        assert!(output.errors[0].contains("live diagnostic"), "{:?}", output.errors);
        assert!(scene.advance(frame.clone()).expect("next frame").errors.is_empty(), "native error is reported once");
    }
    #[test]
    fn live_steps_display_owns_native_children_resources_and_step_lifecycle() {
        let mut compiled=compile_with_context("native-gameplay-steps-display", "return Def.Actor {}", |context| {
            context.defer_initial_live_update=true;
            context.players[0].enabled=true;context.players[1].enabled=true;
            context.song_steps=vec![
                crate::SongLuaStep {steps_type:"StepsType_Dance_Single".into(),difficulty:SongLuaDifficulty::Challenge,
                    description:"P1 chart".into(),chart_name:String::new(),credit:String::new(),meter:9,
                    music_file:String::new(),possible_grade_points:0},
                crate::SongLuaStep {steps_type:"StepsType_Dance_Single".into(),difficulty:SongLuaDifficulty::Edit,
                    description:"Fixture edit".into(),chart_name:String::new(),credit:String::new(),meter:13,
                    music_file:String::new(),possible_grade_points:0}];
            context.player_step_indices=[Some(0),Some(1)];
        });
        let scene=compiled.live_scene.take().expect("owned native StepsDisplays");
        scene.lua.load(r#"
local screen=SCREENMAN:GetTopScreen()
for index,pn in ipairs{PLAYER_1,PLAYER_2} do
    local display=screen:GetChild('StepsDisplayP'..index)
    assert(display:GetNumChildren()==4, 'native StepsDisplay must own its four metric-loaded children')
    assert(display:GetParent()==screen)
    for _,name in ipairs{'Frame','Meter','Description','Autogen'} do
        assert(display:GetChild(name):GetParent()==display, 'native named child ownership')
    end
    assert(display:GetChild('Frame'):GetTexture() and display:GetChild('Autogen'):GetTexture(),
        'inherited graphics redirections load the native blank resources')
    assert(display:GetChild('Meter'):GetText()==(index==1 and '9' or '13'))
    assert(not display:GetChild('Meter'):GetVisible(), 'gameplay MeterOnCommand hides the loaded meter')
    assert(display:GetChild('Description'):GetVisible()==(index==2))
    assert(display:GetChild('Description'):GetText()==(index==1 and 'Expert' or 'Fixture edit'))
    assert(display:SetFromSteps(nil)==display and not display:GetVisible(), 'native Unset keeps the resource tree')
    assert(display:GetNumChildren()==4)
    assert(display:SetFromGameState(pn)==display and display:GetVisible())
end
local p1=screen:GetChild('StepsDisplayP1')
p1:SetFromSteps(GAMESTATE:GetCurrentSteps(PLAYER_2))
assert(p1:GetChild('Meter'):GetText()=='13' and p1:GetChild('Description'):GetText()=='Fixture edit')
assert(p1:GetChild('Description'):GetVisible(), 'native edit description follows selected Steps')
p1:SetFromGameState(PLAYER_1)
assert(p1:GetChild('Meter'):GetText()=='9' and not p1:GetChild('Description'):GetVisible())
"#).exec().expect("native StepsDisplay resources, metric commands and data lifecycle");
    }

    #[test]
    fn live_native_nonfinite_actor_scalar_write_and_immediate_read() {
        let mut compiled = compile_with_context("native-nonfinite-actor-scalars", r#"
local later
return Def.ActorFrame {
    Def.ActorFrame {
        Name='Writer',
        OnCommand=function(self)
            later=self:GetParent():GetChild('Later')
            self:SetUpdateFunction(function()
                -- The original outCirc expression at t=128-122.5, d=2.5
                -- has a negative radicand. Native Actor::SetX preserves it.
                local t=(128-122.5)/2.5-1
                local value=9*math.sqrt(1-math.pow(t,2))+1
                assert(value~=value)
                later:x(value)
                local x=later:GetX()
                assert(x~=x, 'native actor scalar setters must retain NaN, including a later actor in the same update')
                later:zoom(1/x)
                local zoom=later:GetZoom()
                assert(zoom~=zoom, 'native zoom must retain a numeric NaN')
                later:x(math.huge)
                assert(later:GetX()==math.huge, 'native actor float property retains infinity')
                later:x(-math.huge)
                assert(later:GetX()==-math.huge)
            end)
        end,
    },
    Def.Actor {Name='Later',InitCommand=function(self) self:x(7) end},
}
"#, |context| {context.defer_initial_live_update=true;});
        let mut scene = compiled.live_scene.take().expect("native scalar scene");
        let output = scene.advance(SongLuaLiveFrame {music_seconds:0.1,beat:0.2,
            screen_seconds:0.1,delta_seconds:0.1,..Default::default()}).expect("native nonfinite scalar update");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }

    #[test]
    fn live_arrow_rotation_z_uses_native_player_beat_offsets_and_hold_policy() {
        let mut compiled = compile_with_context("native-arrow-rotation-z", "return Def.ActorFrame {}", |context| {
            context.defer_initial_live_update = true;
            for player in &mut context.players { player.enabled = true; }
        });
        let scene = compiled.live_scene.take().unwrap();
        let runtime = scene.lua.globals().raw_get::<Table>(crate::SONG_LUA_RUNTIME_KEY).unwrap();
        runtime.raw_set(crate::SONG_LUA_RUNTIME_BEAT_KEY, 99.0_f32).unwrap();
        let positions = scene.lua.create_table().unwrap();
        for (player, beat) in [std::f32::consts::PI, -std::f32::consts::PI].into_iter().enumerate() {
            let position = scene.lua.create_table().unwrap();
            position.raw_set(crate::SONG_LUA_RUNTIME_BEAT_KEY, beat).unwrap();
            positions.raw_set(player + 1, position).unwrap();
        }
        runtime.raw_set(crate::runtime::PLAYER_SONG_POSITIONS_KEY, positions).unwrap();
        scene.lua.load(r#"
local function near(actual, expected)
    assert(math.abs(actual-expected)<0.0001, tostring(actual)..' vs '..tostring(expected))
end
local ps=GAMESTATE:GetPlayerState(PLAYER_1)
local options=ps:GetCurrentPlayerOptions()
options:Confusion(1)
options:ConfusionOffset(math.pi/2)
options:ConfusionOffset2(math.pi/4)
options:Dizzy(1)
-- At player beat pi: confusion -180, global offset +90, column offset +45.
-- The tap's additional half-pi beat distance contributes +90 dizzy degrees.
near(ArrowEffects.GetRotationZ(ps,math.pi*1.5,false,2),45)
near(ArrowEffects.GetRotationZ(ps,math.pi*1.5,true,2),-45)
options:DizzyHolds(true)
near(ArrowEffects.GetRotationZ(ps,math.pi*1.5,true,2),45)
local other=GAMESTATE:GetPlayerState(PLAYER_2)
other:GetCurrentPlayerOptions():Confusion(1)
near(ArrowEffects.GetRotationZ(other,0,false,1),180)
"#).exec().expect("native confusion, column offset, dizzy hold policy and per-player visible beat");
    }

    #[test]
    fn live_arrow_x_matches_captured_native_source_and_cached_update_phase() {
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../assets/itgmania/arrow-x-native-reference-v1.json")).unwrap();
        let cases = reference["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 130);
        for case in cases {
            let style = case["style"].as_str().unwrap();
            let mut compiled = compile_with_context("native-arrow-x-source", "return Def.ActorFrame {}", |context| {
                context.defer_initial_live_update = true;
                context.style_name = style.into();
                context.screen_height = 480.0;
                for player in &mut context.players { player.enabled = true; }
            });
            let mut scene = compiled.live_scene.take().unwrap();
            let player = case["player"].as_u64().unwrap();
            let options = scene.lua.load(format!(
                "return GAMESTATE:GetPlayerState(PLAYER_{}):GetCurrentPlayerOptions()", player + 1
            )).eval::<Table>().unwrap();
            let song_options = scene.lua.load(format!(
                "return GAMESTATE:GetPlayerState(PLAYER_{}):GetPlayerOptions('ModsLevel_Song')", player + 1
            )).eval::<Table>().unwrap();
            // Native Current approaches the Song target before actor updates.
            // Keep both levels at the same authored values for these math cases.
            for options in [options, song_options] {
            for (name, value) in case["effects"].as_object().unwrap() {
                options.get::<Function>(name.as_str()).unwrap()
                    .call::<Value>((options.clone(), value.as_f64().unwrap())).unwrap();
            }
            options.get::<Function>("Cosecant").unwrap()
                .call::<Value>((options.clone(), case["cosecant"].as_bool().unwrap())).unwrap();
            let timer = ["ModTimerType_Game", "ModTimerType_Beat", "ModTimerType_Song", "ModTimerType_Default"]
                [case["timer"].as_u64().unwrap() as usize];
            options.get::<Function>("ModTimerSetting").unwrap().call::<Value>((options.clone(), timer)).unwrap();
            for (method, name) in [("ModTimerOffset", "timer_offset"), ("ModTimerMult", "timer_mult")] {
                options.get::<Function>(method).unwrap()
                    .call::<Value>((options.clone(), case[name].as_f64().unwrap())).unwrap();
            }
            }
            let output = scene.advance(SongLuaLiveFrame {
                beat: case["beat"].as_f64().unwrap(), music_seconds: case["seconds"].as_f64().unwrap(),
                screen_seconds: 0.1, delta_seconds: 0.1,
                uptime_seconds: case["uptime"].as_f64(), ..Default::default()
            }).unwrap();
            assert!(output.errors.is_empty(), "{}: {:?}", case["id"], output.errors);
            let actual = scene.lua.load(format!(
                "return ArrowEffects.GetXPos(GAMESTATE:GetPlayerState(PLAYER_{}), {}, {})",
                player + 1, case["column"], case["y"]
            )).eval::<f32>().unwrap();
            let expected = &case["expected_x"];
            let valid = match expected.as_str() {
                Some("nan") => actual.is_nan(),
                Some("infinity") => actual == f32::INFINITY,
                Some("-infinity") => actual == f32::NEG_INFINITY,
                _ => (actual - expected.as_f64().unwrap() as f32).abs() <= 0.0001,
            };
            assert!(valid, "native GetXPos must apply source-defined X effects: {} actual={actual:?}, expected={expected}", case["id"]);
        }

        let mut compiled = compile_with_context("native-arrow-x-update-phase", r#"
return Def.ActorFrame {InitCommand=function(self) XROOT=self end}
"#, |context| context.defer_initial_live_update = true);
        let mut scene = compiled.live_scene.take().unwrap();
        scene.lua.load(r#"
PS=GAMESTATE:GetPlayerState(PLAYER_1)
PS:GetCurrentPlayerOptions():Beat(0.05)
PS:GetPlayerOptions('ModsLevel_Song'):Beat(0.05)
assert(ArrowEffects.GetXPos(PS,1,0)==-96, 'native Beat cache begins at zero')
"#).exec().unwrap();
        let frame = |beat| SongLuaLiveFrame { beat, music_seconds: beat / 2.0,
            screen_seconds: beat, delta_seconds: 0.01, ..Default::default() };
        assert!(scene.advance(frame(0.05)).unwrap().errors.is_empty());
        scene.lua.load(r#"
PREVIOUS=ArrowEffects.GetXPos(PS,1,0)
assert(math.abs(PREVIOUS+96)>0.5)
XROOT:SetUpdateFunction(function() DURING=ArrowEffects.GetXPos(PS,1,0) end)
"#).exec().unwrap();
        assert!(scene.advance(frame(0.35)).unwrap().errors.is_empty());
        scene.lua.load(r#"
assert(DURING==PREVIOUS, 'native Beat uses the preceding completed screen update')
assert(ArrowEffects.GetXPos(PS,1,0)==-96, 'native screen update refreshes Beat after actor callbacks')
PS:GetCurrentPlayerOptions():BeatOffset(0.5)
ArrowEffects.Update()
assert(ArrowEffects.GetXPos(PS,1,0)~=-96, 'explicit native Update refreshes effect data')
"#).exec().unwrap();
    }

    #[test]
    fn live_arrow_zoom_matches_native_source_and_column_tiny_lifecycle() {
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../assets/itgmania/arrow-zoom-native-reference-v1.json")).unwrap();
        let cases = reference["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 53);
        for case in cases {
            let column = case["column"].as_u64().unwrap();
            let mut compiled = compile_with_context("native-arrow-zoom-source", "return Def.ActorFrame {}", |context| {
                context.defer_initial_live_update = true;
                context.style_name = if column > 4 { "double" } else { "single" }.into();
            });
            let scene = compiled.live_scene.take().unwrap();
            let options = scene.lua.load(
                "return GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions()"
            ).eval::<Table>().unwrap();
            for (name, value) in case["effects"].as_object().unwrap() {
                options.get::<Function>(name.as_str()).unwrap()
                    .call::<Value>((options.clone(), value.as_f64().unwrap())).unwrap();
            }
            let actual = scene.lua.load(format!(
                "return ArrowEffects.GetZoom(GAMESTATE:GetPlayerState(PLAYER_1), {}, {})",
                case["y"], column
            )).eval::<f32>().unwrap();
            let expected = &case["expected"];
            let valid = match expected.as_str() {
                Some("nan") => actual.is_nan(),
                Some("infinity") => actual == f32::INFINITY,
                Some("-infinity") => actual == f32::NEG_INFINITY,
                _ => (actual - expected.as_f64().unwrap() as f32).abs() <= 0.0001,
            };
            assert!(valid, "native GetZoom must apply per-column Tiny and variable zoom: {} actual={actual:?}, expected={expected}", case["id"]);
        }
        let mut compiled = compile_with_context("native-arrow-zoom-current-lifecycle", "return Def.ActorFrame {}",
            |context| context.defer_initial_live_update = true);
        let mut scene = compiled.live_scene.take().unwrap();
        scene.lua.load(r#"
PS=GAMESTATE:GetPlayerState(PLAYER_1)
SONG=PS:GetPlayerOptions('ModsLevel_Song')
SONG:Tiny4(-0.08752030434412367,10000)
assert(ArrowEffects.GetZoom(PS,150,4)==1, 'Current remains separate until the next update')
"#).exec().unwrap();
        let frame = |time| SongLuaLiveFrame {music_seconds:time,beat:time*2.0,
            screen_seconds:time,delta_seconds:0.1,..Default::default()};
        assert!(scene.advance(frame(0.1)).unwrap().errors.is_empty());
        scene.lua.load(r#"
assert(math.abs(ArrowEffects.GetZoom(PS,150,4)-1.0625423192977905)<0.000001)
assert(ArrowEffects.GetZoom(PS,150,1)==1, 'per-column Tiny does not affect another lane')
SONG:Tiny4(-0.40300090498656843,10000)
assert(math.abs(ArrowEffects.GetZoom(PS,150,4)-1.0625423192977905)<0.000001,
    'native Current still exposes the prior Song target during the setter frame')
"#).exec().unwrap();
        assert!(scene.advance(frame(0.2)).unwrap().errors.is_empty());
        scene.lua.load("assert(ArrowEffects.GetZoom(PS,150,4)>1.3)").exec().unwrap();
    }

    #[test]
    fn live_option_approach_uses_native_frame_delta_at_large_screen_time() {
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../assets/itgmania/native-option-frame-delta-reference-v1.json")).unwrap();
        let cases = reference["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 25);
        for case in cases {
            let mut compiled = compile_with_context("native-option-frame-delta", "return Def.ActorFrame {}",
                |context| context.defer_initial_live_update = true);
            let mut scene = compiled.live_scene.take().unwrap();
            let screen = case["screen"].as_f64().unwrap() as f32;
            let delta = case["delta"].as_f64().unwrap() as f32;
            assert!(scene.advance(SongLuaLiveFrame {
                screen_seconds: f64::from(screen), delta_seconds: 0.0, ..Default::default()
            }).unwrap().errors.is_empty());
            scene.lua.load(format!(r#"
PS=GAMESTATE:GetPlayerState(PLAYER_1)
PS:GetCurrentPlayerOptions():Beat({})
PS:GetPlayerOptions('ModsLevel_Song'):Beat({}, {})
"#,case["initial"],case["target"],case["speed"])).exec().unwrap();
            assert!(scene.advance(SongLuaLiveFrame {
                screen_seconds: f64::from(screen + delta), delta_seconds: f64::from(delta),
                music_seconds: -1.0, ..Default::default()
            }).unwrap().errors.is_empty());
            let actual = scene.lua.load("return PS:GetCurrentPlayerOptions():Beat()")
                .eval::<f32>().unwrap();
            let expected = case["expected"].as_f64().unwrap() as f32;
            assert!((actual-expected).abs() <= 0.0000001,
                "native option approach must use frame delta rather than rounded screen time: {} actual={actual:?} expected={expected:?}",case["id"]);
        }
    }

    #[test]
    fn live_arrow_y_matches_native_source_wave_and_cached_tipsy_phase() {
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../assets/itgmania/arrow-y-native-reference-v1.json")).unwrap();
        let cases = reference["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 63);
        for case in cases {
            let y_pos = case["kind"] == "ypos";
            let mut compiled = compile_with_context("native-arrow-y-source", "return Def.ActorFrame {}", |context| {
                context.defer_initial_live_update = true;
                context.screen_height = 480.0;
                if y_pos { context.style_name = case["style"].as_str().unwrap().into(); }
                for player in &mut context.players { player.enabled = true; }
            });
            let mut scene = compiled.live_scene.take().unwrap();
            let player = case["player"].as_u64().unwrap_or(0) + 1;
            scene.lua.load(format!("PS=GAMESTATE:GetPlayerState(PLAYER_{player})" )).exec().unwrap();
            let current = scene.lua.load("return PS:GetCurrentPlayerOptions()").eval::<Table>().unwrap();
            let song = scene.lua.load("return PS:GetPlayerOptions('ModsLevel_Song')").eval::<Table>().unwrap();
            let expression;
            if y_pos {
                for options in [current, song] {
                    for (name,value) in case["effects"].as_object().unwrap() {
                        options.get::<Function>(name.as_str()).unwrap()
                            .call::<Value>((options.clone(),value.as_f64().unwrap())).unwrap();
                    }
                    for (method,name) in [("Mini","mini"),("Reverse","reverse"),("Centered","centered")] {
                        options.get::<Function>(method).unwrap()
                            .call::<Value>((options.clone(),case[name].as_f64().unwrap())).unwrap();
                    }
                    options.get::<Function>("Cosecant").unwrap()
                        .call::<Value>((options.clone(),case["cosecant"].as_bool().unwrap())).unwrap();
                    let timer=["ModTimerType_Game","ModTimerType_Beat","ModTimerType_Song","ModTimerType_Default"]
                        [case["timer"].as_u64().unwrap() as usize];
                    options.get::<Function>("ModTimerSetting").unwrap().call::<Value>((options.clone(),timer)).unwrap();
                }
                expression=format!("return ArrowEffects.GetYPos(PS, {}, {}, 270)",case["column"],case["y"]);
                // The native binding selects Current options before cached Update.
                scene.lua.load(&expression).eval::<f32>().unwrap();
                if case.get("cache_timer").is_some() {
                    scene.lua.load(r#"
local other=GAMESTATE:GetPlayerState(PLAYER_1)
other:GetCurrentPlayerOptions():ModTimerSetting('ModTimerType_Song')
other:GetPlayerOptions('ModsLevel_Song'):ModTimerSetting('ModTimerType_Song')
ArrowEffects.GetYPos(other,1,0)
"#).exec().unwrap();
                }
                let output=scene.advance(SongLuaLiveFrame {
                    beat:case["beat"].as_f64().unwrap(),music_seconds:case["seconds"].as_f64().unwrap(),
                    screen_seconds:0.1,delta_seconds:0.1,uptime_seconds:case["uptime"].as_f64(),
                    ..Default::default()
                }).unwrap();
                assert!(output.errors.is_empty(),"{}: {:?}",case["id"],output.errors);
            } else {
                for options in [current,song] {
                    for (method,name) in [("Wave","wave"),("WavePeriod","period"),("XMod","speed")] {
                        options.get::<Function>(method).unwrap()
                            .call::<Value>((options.clone(),case[name].as_f64().unwrap())).unwrap();
                    }
                }
                expression=format!("return ArrowEffects.GetYOffset(PS,1,{})",case["note"]);
            }
            let actual=scene.lua.load(&expression).eval::<f32>().unwrap();
            let expected=&case["expected"];
            let valid=match expected.as_str() {
                Some("nan")=>actual.is_nan(),Some("infinity")=>actual==f32::INFINITY,
                Some("-infinity")=>actual==f32::NEG_INFINITY,
                _=>(actual-expected.as_f64().unwrap() as f32).abs()<=0.0001,
            };
            assert!(valid,"native arrow Y must apply Wave and cached Y effects: {} actual={actual:?}, expected={expected}",case["id"]);
        }
        let mut compiled=compile_with_context("native-tipsy-update-phase",r#"
return Def.ActorFrame {InitCommand=function(self) YROOT=self end}
"#,|context|context.defer_initial_live_update=true);
        let mut scene=compiled.live_scene.take().unwrap();
        scene.lua.load(r#"
PS=GAMESTATE:GetPlayerState(PLAYER_1)
PS:GetCurrentPlayerOptions():Tipsy(0.5)
PS:GetPlayerOptions('ModsLevel_Song'):Tipsy(0.5)
BASE=ArrowEffects.GetYPos(PS,1,0)
YROOT:SetUpdateFunction(function() DURING=ArrowEffects.GetYPos(PS,1,0) end)
"#).exec().unwrap();
        let frame=|time,uptime|SongLuaLiveFrame {music_seconds:time,beat:time*2.0,
            screen_seconds:time,delta_seconds:0.1,uptime_seconds:Some(uptime),..Default::default()};
        assert!(scene.advance(frame(0.1,2.0)).unwrap().errors.is_empty());
        scene.lua.load(r#"
assert(DURING==BASE,'native first callback observes the zero Tipsy cache')
PREVIOUS=ArrowEffects.GetYPos(PS,1,0)
assert(math.abs(PREVIOUS-BASE)>1)
"#).exec().unwrap();
        assert!(scene.advance(frame(0.2,2.125)).unwrap().errors.is_empty());
        scene.lua.load(r#"
assert(DURING==PREVIOUS,'native callbacks observe the preceding completed Tipsy update')
assert(ArrowEffects.GetYPos(PS,1,0)~=PREVIOUS,'cache refresh happens after callbacks')
PS:GetPlayerOptions('ModsLevel_Song'):Tipsy(0,10000)
"#).exec().unwrap();
        assert!(scene.advance(frame(0.3,2.25)).unwrap().errors.is_empty());
        scene.lua.load("assert(DURING==BASE and ArrowEffects.GetYPos(PS,1,0)==BASE)").exec().unwrap();
    }
}
