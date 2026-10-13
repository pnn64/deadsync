use mlua::{Function, Lua, MultiValue, Table, Value};
use std::fmt;

use crate::{SongLuaCompileContext, SongLuaOverlayState, SongLuaTrackedActorTarget, LUA_PLAYERS};

pub(crate) const TAP_SCORES: [&str; 11] = [
    "None", "HitMine", "AvoidMine", "CheckpointMiss", "Miss", "W5",
    "W4", "W3", "W2", "W1", "CheckpointHit",
];

pub(crate) struct LiveMode(pub u64);

struct LiveForeground {
    actor: Table,
    start_beat: f32,
    stop_beat: f32,
    started: bool,
    finished: bool,
}

struct LiveBackground {
    actor: Table,
    start_beat: f32,
    started: bool,
}

pub(crate) fn active(lua: &Lua) -> bool {
    lua.app_data_ref::<LiveMode>().is_some()
}

// Trace logging observes the live VM without replacing callbacks or query results.
pub(crate) fn trace_command(lua: &Lua, command: &Function, name: &str, phase: &str) {
    if !active(lua) || !log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) { return; }
    callback_phase(lua, phase == "begin");
    let info = command.info();
    let Some(source) = info.source else { return; };
    if let Ok((beat, seconds)) = crate::compile_song_runtime_values(lua) {
        let scene = lua.app_data_ref::<LiveMode>().map_or(0, |mode| mode.0);
        log::trace!(target: "deadsync_song_lua::trace", "{}", serde_json::json!({
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

pub(crate) fn trace_stage_query(lua: &Lua, operation: &str, points: i64) {
    trace_method(lua, "query", operation, || serde_json::json!({"args":[],"results":[points]}));
}

pub(crate) fn trace_actor_call(lua: &Lua, actor: &Table, operation: &str, arg: Option<&Value>) {
    trace_method(lua, "call", operation, || {
        let arg = match arg {
            Some(Value::Integer(value)) => serde_json::json!(value),
            Some(Value::Number(value)) if value.is_finite() => serde_json::json!(value),
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
    if !active(lua) || !log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) { return; }
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
        let scene = lua.app_data_ref::<LiveMode>().map_or(0, |mode| mode.0);
        let phase = if lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.commands != 0) {
            "callback"
        } else { "construction" };
        let mut event = serde_json::json!({"scene":scene,"kind":kind,"operation":operation,
            "source":source,"line":line,"phase":phase,"beat":beat,"seconds":seconds});
        if let Some(details) = details().as_object() {
            event.as_object_mut().expect("trace event is an object").extend(details.clone());
        }
        log::trace!(target: "deadsync_song_lua::trace", "{event}");
    }
}

// Diagnostic-only registry, owned by the Lua VM. It retains identities and
// original host bindings across screen reloads; no tracing work runs by default.
#[derive(Default)]
struct QueryTrace {
    objects: Vec<(Table, serde_json::Value)>,
    hooks: Vec<(Table, &'static str)>,
    depth: usize,
    commands: usize,
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
    match actor.raw_get::<Option<String>>("__songlua_load_name")? {
        Some(name) => Ok(name),
        None => Ok(actor.raw_get::<Option<String>>("Name")?.unwrap_or_default()),
    }
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
    if lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.hooks.iter().any(|(known, method)|
        known.to_pointer() == table.to_pointer() && *method == name)) { return Ok(()); }
    let original: Function = table.get(name)?;
    table.raw_set(name, lua.create_function(move |lua, args: MultiValue| {
        let observed = lua.app_data_ref::<QueryTrace>().is_some_and(|trace| trace.depth == 0);
        query_phase(lua, true);
        let result = original.call::<MultiValue>(args.clone());
        query_phase(lua, false);
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
    if !active(lua) || !log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) { return Ok(()); }
    if lua.app_data_ref::<QueryTrace>().is_none() { lua.set_app_data(QueryTrace::default()); }
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
}

pub(crate) struct LivePlayers([SongLuaLivePlayer; LUA_PLAYERS]);

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

#[derive(Clone, Copy, Debug, Default)]
pub struct SongLuaLiveFrame {
    pub beat: f64,
    pub music_seconds: f64,
    pub screen_seconds: f64,
    pub delta_seconds: f64,
    /// Independent process clock; music can remain frozen while this advances.
    pub uptime_seconds: Option<f64>,
    /// The gameplay core reached its outro and removed active attacks.
    pub gameplay_outro: bool,
    pub players: [SongLuaLivePlayer; LUA_PLAYERS],
}

impl SongLuaLivePlayer {
    pub fn from_player(player: &deadsync_gameplay::PlayerRuntime, possible_points: i32) -> Self {
        let [w1, w2, w3, w4, w5, miss] = player.judgment_counts;
        Self {
            possible_points, actual_points: player.earned_grade_points,
            current_possible_points: player.current_possible_points,
            life: player.life, failed: player.stage_failed || player.fail_time.is_some(),
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

#[derive(Debug, PartialEq, Eq)]
pub enum SongLuaLiveRequest {
    NewScreen(String),
    Transition(String),
}

pub(crate) struct TransitionRequest(pub String);

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
    pub broadcasts: Vec<(f32, String, bool)>,
    /// Explicit Lua MESSAGEMAN calls, including calls from engine-message
    /// listeners. Native source traces omit the engine-generated messages.
    pub source_broadcasts: Vec<SongLuaLiveBroadcast>,
    pub sounds: Vec<crate::SongLuaSoundEvent>,
    /// Native-style callback errors retain partial state and are reported once.
    pub errors: Vec<String>,
}

impl SongLuaLiveOutput {
    pub fn append(&mut self, mut next: Self) {
        if next.request.is_some() { self.request = next.request; }
        self.broadcasts.append(&mut next.broadcasts);
        self.source_broadcasts.append(&mut next.source_broadcasts);
        self.sounds.append(&mut next.sounds);
        self.errors.append(&mut next.errors);
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
    context: SongLuaCompileContext,
    pub(crate) entry_paths: Vec<std::path::PathBuf>,
    pub(crate) primary_index: usize,
    last_beat: f64,
    last_music_seconds: f32,
    backgrounds: Vec<LiveBackground>,
    foregrounds: Vec<LiveForeground>,
    pub(crate) startup_output: SongLuaLiveOutput,
    outro_started: bool,
    outro_options_reset: bool,
    ended: bool,
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
        let registry = crate::song_lua_actor_registry(&lua)?
            .sequence_values::<Table>().collect::<mlua::Result<Vec<_>>>()?;
        crate::lua_util::prepare_live_updates(&lua, &root)?;
        let states = actors.iter().map(|actor| read_state(&lua, actor))
            .collect::<mlua::Result<Vec<_>>>()?;
        let mut startup_output = take_output(&lua)?;
        startup_broadcasts.append(&mut startup_output.broadcasts);
        startup_output.broadcasts = startup_broadcasts;
        let last_beat = f64::from(crate::compile_song_runtime_values(&lua)?.0);
        let mut backgrounds = Vec::new();
        let mut foregrounds = Vec::new();
        if let Value::Table(roots) = &root {
            for actor in roots.sequence_values::<Value>() {
                let Value::Table(actor) = actor? else { continue; };
                if let Some(start_beat) = actor.raw_get::<Option<f32>>("__songlua_background_start_beat")? {
                    backgrounds.push(LiveBackground { actor: actor.clone(), start_beat, started: false });
                }
                if let Some(start_beat) = actor.raw_get::<Option<f32>>("__songlua_foreground_start_beat")? {
                    foregrounds.push(LiveForeground { actor, start_beat, stop_beat: 0.0, started: false, finished: false });
                }
            }
        }
        Ok(Self { lua, root, stage, players, actors, states, tracked, registry,
            targets: Vec::new(), context, entry_paths, primary_index, last_beat,
            last_music_seconds: -9999.0, backgrounds, foregrounds, startup_output,
            outro_started: false, outro_options_reset: false, ended: false })
    }

    pub fn advance(&mut self, frame: SongLuaLiveFrame) -> mlua::Result<SongLuaLiveOutput> {
        if self.ended { return Err(mlua::Error::runtime("cannot advance an ended song Lua scene")); }
        // GameState.cpp::UpdateSongPosition maintains global Song timing apart
        // from each Player's selected Steps timing. The domain frame carries
        // the player's beat; Lua's GAMESTATE and music effects use the song.
        let position = self.context.song_timing.as_ref()
            .map(|timing| timing.get_song_position(frame.music_seconds as f32));
        let beat = position.as_ref().map_or(frame.beat, |position| f64::from(position.beat));
        self.feed_stats(&frame)?;
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
        }
        crate::set_compile_song_runtime_delta_values(&self.lua,
            (beat - self.last_beat) as f32, frame.delta_seconds as f32)?;
        self.last_beat = beat;
        crate::lua_util::set_live_frame(&self.lua, frame.screen_seconds, beat, frame.delta_seconds);
        self.lua.set_app_data(LivePlayers(frame.players));
        crate::song_tables::advance_option_levels(&self.lua, frame.screen_seconds)?;
        self.advance_backgrounds(beat as f32, frame.music_seconds as f32)?;
        self.advance_foregrounds(beat as f32, frame.music_seconds as f32)?;
        crate::lua_util::run_actor_compile_update_functions_with_delta(
            &self.lua, &self.root, frame.delta_seconds)?;
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
        if self.outro_started && !self.outro_options_reset {
            crate::song_tables::clear_active_attack_options(&self.lua)?;
            self.outro_options_reset = true;
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
        for background in &mut self.backgrounds {
            let inactive = beat < background.start_beat;
            background.actor.raw_set("__songlua_live_inactive", inactive)?;
            if inactive { continue; }
            let delta = if !background.started {
                // BackgroundImpl::Layer::UpdateCurBGChange sends On when the
                // music clock reaches this change, then consumes the elapsed
                // music since its authored start before the regular updates.
                background.actor.raw_set("__songlua_defer_on", false)?;
                crate::lua_util::run_actor_startup_commands_for_table(&self.lua, &background.actor)?;
                background.actor.get::<Function>("playcommand")?
                    .call::<()>((background.actor.clone(), "GainFocus"))?;
                background.started = true;
                let start = self.context.song_timing.as_ref().map_or_else(
                    || crate::song_elapsed_seconds_at(background.start_beat, &self.context) * crate::song_music_rate(&self.context),
                    |timing| timing.get_time_for_beat_exact(background.start_beat));
                (music_seconds - start).max(0.0) / rate
            } else {
                (music_seconds - self.last_music_seconds).max(0.0) / rate
            };
            background.actor.raw_set("__songlua_live_delta", f64::from(delta))?;
        }
        Ok(())
    }

    fn advance_foregrounds(&mut self, beat: f32, music_seconds: f32) -> mlua::Result<()> {
        if self.foregrounds.is_empty() { return Ok(()); }
        let prefs = self.lua.globals().get::<Table>("PREFSMAN")?;
        let rate_tweens = prefs.get::<Function>("GetPreference")?
            .call::<Option<bool>>((prefs, "RateModsAffectFGChanges"))?.unwrap_or(false);
        let game = self.lua.globals().get::<Table>("GAMESTATE")?;
        let options = game.get::<Function>("GetSongOptionsObject")?
            .call::<Table>((game, "ModsLevel_Current"))?;
        let rate = if rate_tweens { 1.0 } else { options.get::<Function>("MusicRate")?.call::<f32>(options)? };
        for foreground in &mut self.foregrounds {
            let inactive = foreground.finished || beat < foreground.start_beat;
            foreground.actor.raw_set("__songlua_live_inactive", inactive)?;
            if inactive { continue; }
            let visible = foreground.actor.get::<Function>("GetVisible")?.call::<bool>(foreground.actor.clone())?;
            let delta = if !visible {
                foreground.actor.get::<Function>("visible")?.call::<()>((foreground.actor.clone(), true))?;
                foreground.actor.raw_set("__songlua_defer_on", false)?;
                if foreground.started {
                    foreground.actor.get::<Function>("playcommand")?.call::<()>((foreground.actor.clone(), "On"))?;
                } else {
                    crate::lua_util::run_actor_startup_commands_for_table(&self.lua, &foreground.actor)?;
                }
                let start = self.context.song_timing.as_ref().map_or_else(
                    || crate::song_elapsed_seconds_at(foreground.start_beat, &self.context) * crate::song_music_rate(&self.context),
                    |timing| timing.get_time_for_beat_exact(foreground.start_beat));
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
        self.stage.raw_set("__songlua_gameplay_seconds", frame.screen_seconds)?;
        for (stats, value) in self.players.iter().zip(frame.players) {
            stats.raw_set("__songlua_possible_dance_points", value.possible_points)?;
            stats.raw_set("__songlua_actual_dance_points", value.actual_points)?;
            stats.raw_set("__songlua_current_possible_dance_points", value.current_possible_points)?;
            stats.raw_set("__songlua_failed", value.failed)?;
            stats.raw_set("__songlua_life", value.life)?;
            stats.raw_set("__songlua_combo", value.combo)?;
            stats.raw_set("__songlua_miss_combo", value.miss_combo)?;
            let counts = stats.raw_get::<Table>("__songlua_tap_scores")?;
            for (name, count) in TAP_SCORES.into_iter().zip(value.tap_scores) {
                counts.raw_set(name, count)?;
            }
        }
        Ok(())
    }

    pub fn context(&self) -> &SongLuaCompileContext { &self.context }

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
        self.targets = targets;
        Ok(())
    }

    /// Apply current local poses. The screen marks these actors dynamic in
    /// FrameScratch at entry, before rendering starts; baked timelines stay empty.
    pub fn write_visuals<S>(
        &self,
        visuals: &mut deadsync_gameplay::SongLuaRuntimeVisuals<
            crate::SongLuaOverlayActor<S>, crate::SongLuaCapturedActor,
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

    pub fn states(&self) -> &[SongLuaOverlayState] { &self.states }

    pub fn tracked_states(&self) -> impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)> + '_ {
        self.tracked.iter().map(|(target, _, state)| (*target, *state))
    }

    pub fn take_startup_output(&mut self) -> SongLuaLiveOutput {
        std::mem::take(&mut self.startup_output)
    }
}

fn take_output(lua: &Lua) -> mlua::Result<SongLuaLiveOutput> {
    let request = if let Some(request) = lua.remove_app_data::<crate::host::SongLuaScreenRequest>() {
        Some(SongLuaLiveRequest::NewScreen(request.0))
    } else {
        lua.remove_app_data::<TransitionRequest>().map(|request| SongLuaLiveRequest::Transition(request.0))
    };
    let broadcasts = crate::lua_util::take_runtime_broadcasts(lua);
    let source_broadcasts = crate::lua_util::take_source_broadcasts(lua);
    let sounds = crate::lua_util::take_runtime_sounds(lua);
    let errors = lua.remove_app_data::<crate::lua_util::SongLuaUpdateErrors>()
        .map_or_else(Vec::new, |errors| errors.0);
    if let Some(history) = lua.globals().raw_get::<Option<Table>>(crate::SONG_LUA_BROADCASTS_KEY)? {
        history.clear()?;
    }
    Ok(SongLuaLiveOutput { request, broadcasts, source_broadcasts, sounds, errors })
}

pub(crate) fn read_state(lua: &Lua, actor: &Table) -> mlua::Result<SongLuaOverlayState> {
    let mut state = crate::actor_overlay_initial_state(actor).map_err(mlua::Error::external)?;
    crate::lua_util::replay_tween_pose(lua, actor, &mut state);
    state.spin_baked = crate::lua_util::spin_render_pose(actor)?.is_some();
    Ok(state)
}

#[cfg(test)]
mod tests {
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
        let dir = std::env::temp_dir().join(format!("deadsync-live-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("test directory");
        let entry = dir.join("default.lua");
        fs::write(&entry, source).expect("test script");
        let mut context = SongLuaCompileContext::new(&dir, name);
        context.live = true;
        context.music_length_seconds = 60.0;
        crate::compile_song_lua_with_default_host(&entry, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("live preparation")
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
            scene.advance(frame).expect("outro frame");
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
        scene.advance(frame).expect("continued outro");
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
        scene.advance(input).expect("first judgment");
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
        scene.advance(input).expect("existing judgment advances");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &player).expect("P1 time") - 0.8375).abs() < 1e-6,
            "the named child advances once and the unchanged snapshot does not restart it");
        deadsync_gameplay::set_player_last_judgment(&mut runtime,
            deadsync_rules::judgment::Judgment { grade: deadsync_rules::judgment::JudgeGrade::Miss, window: None, ..judgment }, 0.25);
        input.players[0] = SongLuaLivePlayer::from_player(&runtime, 100);
        input.screen_seconds = 0.25;
        scene.advance(input).expect("replacement judgment");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("sprite time") - 0.9).abs() < 1e-6);
        assert_eq!(sprite.get::<Function>("GetState").expect("state getter").call::<usize>(sprite.clone()).expect("miss frame"), 5);

        player.get::<Function>("hibernate").expect("hibernate").call::<()>((player.clone(), 0.125)).expect("pause player");
        input.screen_seconds += 0.0625;
        scene.advance(input).expect("hibernating player");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("paused child") - 0.9).abs() < 1e-6);
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &player).expect("paused parent") - 0.9625).abs() < 1e-6);
        input.screen_seconds += 0.125;
        input.delta_seconds = 0.125;
        scene.advance(input).expect("wake remainder");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("wake child") - 0.8375).abs() < 1e-6);
        player.get::<Function>("SetUpdateRate").expect("update rate").call::<()>((player.clone(), 2.0)).expect("double rate");
        input.screen_seconds += 0.0625;
        input.delta_seconds = 0.0625;
        scene.advance(input).expect("rated child update");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("rated child") - 0.7125).abs() < 1e-6);
    }

    #[test]
    fn live_player_hold_judgment_tree() {
        let mut compiled = compile("player-hold-judgment-tree", r#"
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
local holds = player:GetChildren()['']
local named = player:GetChild('')
assert(#holds == 4 and #named == 4 and named[1] == holds[1])
assert(player:GetNumChildren() == 7)
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
        scene.advance(input).expect("held feedback without tap feedback or scoring");
        assert!((crate::lua_util::actor_tween_time_left(&scene.lua, &sprite).expect("hold queue") - 0.8).abs() < 1e-6);
        assert!((scene.states()[0].aux - 0.8).abs() < 1e-6, "authored callbacks see hold descendants");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &other).expect("P2 queue"), 0.0);
        let mut pose = SongLuaOverlayState::default();
        assert!(crate::lua_util::replay_tween_pose(&scene.lua, &sprite, &mut pose));
        assert!((pose.zoom_x - 0.4).abs() < 1e-6, "new queue does not consume the current frame");
        assert_eq!(pose.diffuse[3], 1.0);

        input.screen_seconds += 0.0625;
        input.delta_seconds = 0.0625;
        scene.advance(input).expect("unchanged hold feedback");
        assert!((scene.states()[0].aux - 0.7375).abs() < 1e-6, "the snapshot does not restart the queue");
        input.players[0].hold_judgments[1] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
            result: deadsync_rules::note::HoldResult::LetGo, started_at_screen_s: 0.25,
        });
        scene.advance(input).expect("let-go replaces the old queue");
        assert!((scene.states()[0].aux - 0.8).abs() < 1e-6);
        assert_eq!(sprite.get::<Function>("GetState").expect("state getter")
            .call::<usize>(sprite.clone()).expect("let-go frame"), 1);
        input.players[0].hold_judgments[1] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
            result: deadsync_rules::note::HoldResult::Missed, started_at_screen_s: 0.3125,
        });
        scene.advance(input).expect("native empty MissedHold command");
        assert!((scene.states()[0].aux - 0.7375).abs() < 1e-6);
        input.players[0].hold_judgments.fill(None);
        input.delta_seconds = 1.0;
        scene.advance(input).expect("feedback expires independently of the snapshot");
        assert_eq!(crate::lua_util::actor_tween_time_left(&scene.lua, &player).expect("finished player"), 0.0);
    }

    #[test]
    fn live_named_notefield_advances_once() {
        let mut compiled = compile("named-notefield-once", r#"
local field = SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
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
        let output = scene.advance(frame).expect("first half frame");
        assert!(output.errors.is_empty() && output.broadcasts.is_empty());
        assert!((scene.states()[global].x - 50.0).abs() < 1e-5);
        player.current_possible_points = 10;
        frame.players[0] = SongLuaLivePlayer::from_player(&player, 10);
        frame.screen_seconds = 1.0;
        let output = scene.advance(frame).expect("queue completion");
        assert!(output.errors.is_empty() && output.broadcasts.is_empty());
        assert_eq!(scene.states()[global].x, 100.0);
        // Actor.cpp::UpdateTweening stops when the positive delta is exhausted.
        // A trailing zero-time command runs on the next positive update.
        frame.screen_seconds = 1.1;
        frame.delta_seconds = 0.1;
        let output = scene.advance(frame).expect("trailing command");
        assert!(output.errors.is_empty());
        assert_eq!(output.broadcasts.len(), 1);
        assert_eq!(output.broadcasts[0].1, "ActualTick");
        assert_eq!(scene.states()[global].aux, 10.0);
        frame.screen_seconds = 1.2;
        assert!(scene.advance(frame).expect("later frame").broadcasts.is_empty(), "drain events exactly once");
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
        let output = scene.advance(frame).expect("first foreground On");
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
        assert!(scene.advance(frame).expect("shared update").errors.is_empty());
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
        let output = scene.advance(frame).expect("native partial callback");
        assert_eq!(output.request, Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into())));
        assert_eq!(output.errors.len(), 1);
        assert!(output.errors[0].contains("live diagnostic"), "{:?}", output.errors);
        assert!(scene.advance(frame).expect("next frame").errors.is_empty(), "native error is reported once");
    }
}
