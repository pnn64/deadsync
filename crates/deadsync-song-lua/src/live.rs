#[cfg(any(test, feature = "legacy-oracle"))]
use mlua::{Function, Lua, MultiValue, Table, Value};

#[cfg(test)]
#[path = "../tests/perf/forward_trace.rs"]
mod forward_trace_perf;
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
struct LiveForeground {
    actor: Table,
    start_beat: f32,
    stop_beat: f32,
    started: bool,
    finished: bool,
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn active(lua: &Lua) -> bool {
    lua.app_data_ref::<LiveMode>().is_some()
}

// The song observer starts after screen/theme construction, including when
// a retained VM replaces its screen. Keep the actual native callbacks running.
#[cfg(any(test, feature = "legacy-oracle"))]
fn query_trace_ready(lua: &Lua) -> bool {
    let Some(scene) = lua.app_data_ref::<LiveMode>().map(|mode| mode.0) else { return false; };
    lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.scene == scene)
}

// Native SongLuaTrace.lua wraps authored callbacks only. Reserved theme chunks
// run normally; nested calls remain visible under an existing song command.
#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn observes_song_callback(callback: &Function) -> bool {
    !callback.info().source.is_some_and(|source| source.starts_with("@theme:"))
}

// Trace logging observes the live VM without replacing callbacks or query results.
#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn callback_phase(lua: &Lua, entering: bool) {
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() {
        if entering { trace.commands += 1; }
        else { trace.commands = trace.commands.saturating_sub(1); }
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn trace_stage_query(lua: &Lua, operation: &str, points: i64) {
    trace_method(lua, "query", operation, || serde_json::json!({"args":[],"results":[points]}));
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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
#[cfg(any(test, feature = "legacy-oracle"))]
#[derive(Default)]
struct QueryTrace {
    scene: u64,
    objects: Vec<(Table, serde_json::Value)>,
    hooks: Vec<(Table, &'static str)>,
    depth: usize,
    commands: usize,
    dynamic_commands: bool,
}

#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(feature = "test-support")]
pub(crate) fn set_dynamic_command_observer(lua: &Lua, enabled: bool) {
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() {
        trace.dynamic_commands = enabled;
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn observes_dynamic_commands(lua: &Lua) -> bool {
    lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.dynamic_commands)
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
fn query_actor_name(actor: &Table) -> mlua::Result<String> {
    crate::lua_util::actor_name(actor)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn query_details(lua: &Lua, args: &MultiValue, result: &mlua::Result<MultiValue>) -> mlua::Result<serde_json::Value> {
    let args = args.iter().skip(1).map(|arg| query_value(lua, arg, 0))
        .collect::<mlua::Result<Vec<_>>>()?;
    let values = result.as_ref().map_err(Clone::clone)?;
    let results = values.iter().map(|value| query_value(lua, value, 0))
        .collect::<mlua::Result<Vec<_>>>()?;
    Ok(serde_json::json!({"args":args,"results":results}))
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn query_phase(lua: &Lua, entering: bool) {
    if let Some(mut trace) = lua.app_data_mut::<QueryTrace>() {
        if entering { trace.depth += 1; }
        else { trace.depth = trace.depth.saturating_sub(1); }
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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
#[cfg(any(test, feature = "legacy-oracle"))]
struct LiveStats([Table; LUA_PLAYERS]);
#[derive(Default)]
struct HealthStates([Option<&'static str>; LUA_PLAYERS]);

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn bind_player_stats(lua: &Lua, players: [Table; LUA_PLAYERS]) {
    lua.set_app_data(LiveStats(players));
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn queue_player_write(lua: &Lua, write: SongLuaLivePlayerWrite) {
    if !active(lua) { return; }
    if let Some(mut writes) = lua.app_data_mut::<PlayerWrites>() {
        writes.0.push(write);
    } else {
        lua.set_app_data(PlayerWrites(vec![write]));
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn player_life(lua: &Lua, player: usize) -> Option<f32> {
    lua.app_data_ref::<LivePlayers>().and_then(|players| players.0.get(player).map(|p| p.life))
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn player_autoplay(lua: &Lua, player: usize) -> Option<bool> {
    lua.app_data_ref::<LivePlayers>().and_then(|players| players.0.get(player).map(|p| p.autoplay))
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn player_health_state(lua: &Lua, player: usize) -> Option<&'static str> {
    lua.app_data_ref::<HealthStates>().and_then(|states| states.0.get(player).copied().flatten())
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn fail_player(lua: &Lua, player: usize) {
    if let Some(mut values) = lua.app_data_mut::<LivePlayers>()
        && let Some(value) = values.0.get_mut(player) { value.failed = true; }
    queue_player_write(lua, SongLuaLivePlayerWrite::Fail { player });
}

pub(crate) fn apply_player_writes<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
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

pub(crate) fn judgment_score(judgment: deadsync_rules::judgment::Judgment) -> &'static str {
    use deadsync_rules::judgment::JudgeGrade::*;
    match judgment.grade {
        Fantastic => "TapNoteScore_W1", Excellent => "TapNoteScore_W2",
        Great => "TapNoteScore_W3", Decent => "TapNoteScore_W4",
        WayOff => "TapNoteScore_W5", Miss => "TapNoteScore_Miss",
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
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
    let participant = note.participant;
    table.raw_set("GetPlayerNumber", lua.create_function(move |_, _: MultiValue| {
        Ok(participant.map(|owner| if owner == 0 { "PlayerNumber_P1" } else { "PlayerNumber_P2" }))
    })?)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn capture_source_request(lua: &Lua, request: SongLuaLiveRequest) -> mlua::Result<()> {
    if lua.app_data_ref::<SourceRequests>().is_none() { return Ok(()); }
    let music_seconds = f64::from(crate::compile_song_runtime_values(lua)?.1);
    if let Some(mut events) = lua.app_data_mut::<SourceRequests>() {
        events.0.push(SongLuaLiveSourceRequest { music_seconds, request });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
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
    /// Ordered actor-owned audio operations emitted by authored Lua callbacks.
    pub actor_sounds: Vec<SongLuaActorSound>,
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
        self.actor_sounds.append(&mut next.actor_sounds);
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

#[derive(Debug, Clone, PartialEq)]
pub enum SongLuaActorSound {
    Play { owner: u64, path: std::path::PathBuf },
    Stop { owner: u64 },
    Pause { owner: u64, paused: bool },
}

#[derive(Default)]
struct ActorSounds(Vec<SongLuaActorSound>);

#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_take_actor_sounds(lua: &Lua) -> Vec<SongLuaActorSound> {
    lua.remove_app_data::<ActorSounds>().map_or_else(Vec::new, |sounds| sounds.0)
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn actor_sound(lua: &Lua, command: SongLuaActorSound) {
    if !active(lua) { return; }
    if let Some(mut sounds) = lua.app_data_mut::<ActorSounds>() {
        sounds.0.push(command);
    } else {
        lua.set_app_data(ActorSounds(vec![command]));
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
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

#[cfg(any(test, feature = "legacy-oracle"))]
impl fmt::Debug for SongLuaLiveScene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SongLuaLiveScene")
            .field("actors", &self.actors.len())
            .field("tracked", &self.tracked.len())
            .finish_non_exhaustive()
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
impl SongLuaLiveScene {
    /// Lua allocator bytes currently retained by this scene's VM. This excludes
    /// Rust actor state, native compiled code and allocator bookkeeping.
    pub fn lua_memory_bytes(&self) -> usize { self.lua.used_memory() }

    /// Compile a snapshot of the screen's input interactions and actor states.
    /// The returned owner processes future inputs without Lua. This covers only
    /// input callbacks: updates, timers, drawing callbacks and other scene
    /// lifecycle behavior remain owned by this live scene.
    /// States and captured actor bindings are copied at this point. Subsequent
    /// live scene activity is independent of this snapshot.
    ///
    /// Captured actors, raw scalar tables shared by callbacks and authenticated
    /// native `visible(boolean)` calls are supported. Globals, scalar upvalues,
    /// event mutation and other
    /// methods reject the whole input snapshot without executing callbacks.
    pub fn compile_actor_inputs(&self) -> Option<crate::actor_input::SongLuaCompiledActorInputs> {
        if self.ended { return None; }
        crate::actor_input::compile(&self.lua, &self.actors, &self.states, &self.tracked)
    }

    /// Snapshot input and authored-root update callbacks into one native unit.
    /// Captured scalar tables are shared across both callback kinds. This owner
    /// is independent of later live-scene lifecycle work; it does not replace
    /// the scene's host updates, queued commands, drawing or foreground timing.
    pub fn compile_actor_interactions(&self) -> Option<crate::actor_input::SongLuaCompiledActorInteractions> {
        if self.ended { return None; }
        crate::actor_input::update::compile(&self.lua, &self.root, &self.actors, &self.states, &self.tracked)
    }

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
        #[cfg(feature = "performance")]
        let state_read_profile = crate::live_profile::scope(crate::live_profile::Phase::StateRead);
        for (actor, state) in self.actors.iter().zip(&mut self.states) {
            *state = read_state(&self.lua, actor)?;
        }
        for (_, actor, state) in &mut self.tracked { *state = read_state(&self.lua, actor)?; }
        #[cfg(feature = "performance")]
        drop(state_read_profile);
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
        crate::lua_util::begin_live_actor_playback(&lua, &registry)?;
        Ok(Self { lua, root, stage, players, actors, states, tracked,
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
        #[cfg(feature = "performance")]
        let actor_update_profile = crate::live_profile::scope(crate::live_profile::Phase::ActorUpdate);
        crate::lua_util::run_actor_compile_update_functions_with_entry(
            &lua, &root, frame.delta_seconds, |actor| {
                // Foreground is sorted after the screen's underlay and Player
                // actors. Its On callbacks observe their current-frame queues.
                self.advance_foreground(actor, beat as f32, frame.music_seconds as f32)
            })?;
        #[cfg(feature = "performance")]
        drop(actor_update_profile);
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
        #[cfg(feature = "performance")]
        let state_read_profile = crate::live_profile::scope(crate::live_profile::Phase::StateRead);
        for (actor, state) in self.actors.iter().zip(&mut self.states) {
            *state = read_state(&self.lua, actor)?;
        }
        for (_, actor, state) in &mut self.tracked {
            *state = read_state(&self.lua, actor)?;
        }
        #[cfg(feature = "performance")]
        drop(state_read_profile);
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
            #[cfg(feature = "performance")]
            let _actor_draw_profile = crate::live_profile::scope(crate::live_profile::Phase::ActorDraw);
            crate::lua_util::run_actor_draw_functions_for_table(&self.lua, root)?;
        }
        #[cfg(feature = "performance")]
        let state_read_profile = crate::live_profile::scope(crate::live_profile::Phase::StateRead);
        for (actor, state) in self.actors.iter().zip(&mut self.states) {
            *state = read_state(&self.lua, actor)?;
        }
        for (_, actor, state) in &mut self.tracked {
            *state = read_state(&self.lua, actor)?;
        }
        #[cfg(feature = "performance")]
        drop(state_read_profile);
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
        for actor in &self.actors {
            if crate::lua_util::actor_type_is(actor, "Sound")? {
                actor_sound(&self.lua, SongLuaActorSound::Stop { owner: actor.to_pointer() as u64 });
            }
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
        #[cfg(feature = "performance")]
        let _visual_profile = crate::live_profile::scope(crate::live_profile::Phase::VisualPublication);
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

#[cfg(any(test, feature = "legacy-oracle"))]
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
            crate::text_publication::sync_text(owned, text).map_err(|error| error.to_string())?;
            crate::text_publication::sync_attributes(owned, attributes)?;
            *stroke_color = crate::lua_util::read_actor_color_field(owned, "__songlua_stroke_color")?;
        }
        crate::SongLuaOverlayKind::Sprite { texture_path, texture_key, states, .. } => {
            if let Some(path) = crate::lua_util::actor_texture_path(owned).map_err(|error| error.to_string())?
                && *texture_path != path
            {
                *texture_key = std::sync::Arc::from(path.to_string_lossy().as_ref());
                *texture_path = path;
            }
            crate::lua_util::sync_sprite_states(owned, states)?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(any(test, feature = "legacy-oracle"))]
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
    let actor_sounds = lua.remove_app_data::<ActorSounds>().map_or_else(Vec::new, |sounds| sounds.0);
    let errors = lua.remove_app_data::<crate::lua_util::SongLuaUpdateErrors>()
        .map_or_else(Vec::new, |errors| errors.0);
    let coverage_gaps = crate::lua_util::take_message_coverage(lua);
    if let Some(history) = lua.globals().raw_get::<Option<Table>>(crate::SONG_LUA_BROADCASTS_KEY)? {
        history.clear()?;
    }
    let player_writes = lua.remove_app_data::<PlayerWrites>().map_or_else(Vec::new, |writes| writes.0);
    let draw_frames = crate::draw_capture::take_live(lua).map_err(mlua::Error::external)?;
    Ok(SongLuaLiveOutput { request, source_requests, broadcasts, source_broadcasts, sounds, actor_sounds, errors, coverage_gaps, player_writes, draw_frames })
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn read_state(lua: &Lua, actor: &Table) -> mlua::Result<SongLuaOverlayState> {
    let mut state = crate::lua_util::live_actor_overlay_state(lua, actor).map_err(mlua::Error::external)?;
    state.spin_baked = crate::lua_util::spin_render_pose(actor)?.is_some();
    Ok(state)
}



// Publish phase results for migration controls without running the live scene.
#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(feature = "test-support")]
pub(crate) fn migration_publish_player_status(
    lua: &Lua,
    autoplay: Option<[bool; LUA_PLAYERS]>,
    health: [Option<&'static str>; LUA_PLAYERS],
) {
    if let Some(autoplay) = autoplay {
        lua.set_app_data(LivePlayers(autoplay.map(|autoplay| SongLuaLivePlayer {
            autoplay,
            ..SongLuaLivePlayer::default()
        })));
    } else {
        lua.remove_app_data::<LivePlayers>();
    }
    lua.set_app_data(HealthStates(health));
}

#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_feed_statistics(
    stage: &Table,
    players: &[Table; LUA_PLAYERS],
    frame: &SongLuaLiveFrame,
) -> mlua::Result<()> {
    feed_live_stats(stage, players, frame)
}

#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_publish_live_players(lua: &Lua, players: [SongLuaLivePlayer; LUA_PLAYERS]) {
    lua.set_app_data(LivePlayers(players));
}

#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_take_player_writes(lua: &Lua) -> Vec<SongLuaLivePlayerWrite> {
    lua.remove_app_data::<PlayerWrites>()
        .map_or_else(Vec::new, |writes| writes.0)
}

#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_live_players(lua: &Lua) -> Option<[SongLuaLivePlayer; LUA_PLAYERS]> {
    lua.app_data_ref::<LivePlayers>().map(|players| players.0)
}

#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_take_output(lua: &Lua) -> mlua::Result<SongLuaLiveOutput> {
    take_output(lua)
}

#[cfg(any(test, feature = "legacy-oracle"))]
#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
pub(crate) fn migration_finish_health_update(lua: &Lua) -> mlua::Result<()> {
    finish_health_update(lua)
}
