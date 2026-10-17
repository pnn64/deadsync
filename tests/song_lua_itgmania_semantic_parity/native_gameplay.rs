//! Replay actual native clock inputs through the owned gameplay Lua screen.
//! Native expectations are observations; they are never fed into gameplay.
use crate::paths;

use deadsync_gameplay::{
    GameplayAudioSnapshot, GameplayConfig, GameplaySession, GameplayStreamClockSnapshot,
    GameplayViewport,
};
use deadsync_theme_simply_love::screens::gameplay;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, OnceLock},
};

#[path = "interactive.rs"]
mod interactive;

const CALL_OPERATIONS: &[&str] = &[
    "command.begin",
    "command.end",
    "PlayerStageStats.GetCurrentPossibleDancePoints",
    "PlayerStageStats.GetPossibleDancePoints",
    "Actor.sleep",
    "Actor.queuecommand",
    "GameState.GetCurrentSong",
    "Song.GetStepsByStepsType",
    "GameState.IsPlayerEnabled",
    "GameState.GetCurrentSteps",
    "Steps.GetDifficulty",
    "GameState.SetCurrentSteps",
    "StatsManager.GetCurStageStats",
    "StageStats.GetPlayerStageStats",
    "ScreenManager.GetTopScreen",
    "GameState.GetCurrentStyle",
    "GameState.GetPlayerState",
    "GameState.GetSongBeat",
    "Steps.GetStepsType",
    "Style.GetName",
    "Song.GetSongDir",
    "PrefsManager.GetPreference",
    "RageDisplay.SupportsRenderToTexture",
    "PlayerState.GetPlayerOptions",
    "PlayerState.GetPlayerOptionsString",
    "PlayerState.SetPlayerOptions",
    "GameState.GetSongOptionsObject",
    "SongOptions.MusicRate",
    "PlayerOptions.Reverse",
    "PlayerOptions.Boost",
    "PlayerOptions.ConfusionOffset",
    "PlayerOptions.Tiny1",
    "PlayerOptions.Tiny2",
    "PlayerOptions.Tiny3",
    "PlayerOptions.Tiny4",
    "ActorFrame.GetChild",
    "ActorFrame.GetNumChildren",
    "Actor.GetName",
    "Actor.GetParent",
    "Actor.name",
    "Actor.aux",
    "Actor.getaux",
    "Actor.x",
    "Actor.y",
    "Actor.zoomx",
    "Actor.vibrate",
    "Actor.effectmagnitude",
    "Actor.Center",
    "Actor.FullScreen",
    "Actor.animate",
    "Actor.decelerate",
    "Actor.diffuse",
    "Actor.diffusealpha",
    "Actor.effectperiod",
    "Actor.linear",
    "Actor.queuemessage",
    "Actor.rainbow",
    "Actor.rotationz",
    "Actor.spin",
    "Actor.visible",
    "Actor.zoom",
    "Actor.zoomto",
    "ActorFrame.SetUpdateFunction",
    "ActorProxy.SetTarget",
    "Sprite.GetTexture",
    "Sprite.loop",
    "Sprite.rate",
];

struct CallLog;
static CALL_LOG: CallLog = CallLog;
struct CallEvents {
    events: Vec<Value>,
    dropped: usize,
    limit: usize,
}
thread_local! {
    // Each owned Lua VM runs on its replay thread. Unrelated test threads
    // must neither clear its observations nor consume its trace capacity.
    static CALL_EVENTS: RefCell<CallEvents> = const {
        RefCell::new(CallEvents { events: Vec::new(), dropped: 0, limit: 0 })
    };
}

impl log::Log for CallLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        (metadata.target() == "deadsync_song_lua::trace"
            || metadata.level() == log::Level::Warn
                && metadata.target().starts_with("deadsync_song_lua::"))
            && CALL_EVENTS.try_with(|capture| capture.borrow().limit > 0).unwrap_or(false)
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // Keep the real preparation error visible when the shared-session
        // fallback cannot retain a scene. Warnings are diagnostic output,
        // never command observations or new comparison requirements.
        if record.level() == log::Level::Warn {
            eprintln!("{}: {}", record.target(), record.args());
            return;
        }
        record_call_event(serde_json::from_str(&record.args().to_string())
            .unwrap_or_else(|error| json!({"kind":"trace-error","error":error.to_string()})));
    }
    fn flush(&self) {}
}

fn record_call_event(event: Value) {
    CALL_EVENTS.with_borrow_mut(|capture| {
        if capture.events.len() >= capture.limit {
            capture.dropped += 1;
        } else {
            capture.events.push(event);
        }
    });
}

fn start_call_log(limit: usize) -> Result<(), String> {
    if limit == 0 { return Err("call trace capacity must be positive".into()); }
    static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| log::set_logger(&CALL_LOG).map_err(|error| error.to_string()))
        .clone()?;
    CALL_EVENTS.with_borrow_mut(|capture| {
        capture.events.clear();
        capture.dropped = 0;
        capture.limit = limit;
    });
    log::set_max_level(log::LevelFilter::Trace);
    deadsync_song_lua::trace_capture::set_sink(Some(record_call_event));
    Ok(())
}

fn compare_calls(
    trace: &Value,
    simfile: &Path,
    scene: u64,
    seconds: f64,
    scope: Scope,
    mismatches: &mut Vec<Value>,
) -> Result<Value, String> {
    let song_dir = format!(
        "{}/",
        simfile
            .parent()
            .ok_or("missing song directory")?
            .to_string_lossy()
            .replace('\\', "/")
    );
    let (actual, dropped, uncompared_actual) = CALL_EVENTS.with_borrow_mut(|capture| {
        for event in capture.events.iter().filter(|event| event["kind"] == "trace-error") {
            mismatches.push(json!({"field":"call_trace_error","deadsync":event}));
        }
        let mut actual_handles = BTreeMap::new();
        let mut uncompared = Vec::new();
        let actual: Vec<Value> = std::mem::take(&mut capture.events)
            .into_iter()
            .filter_map(|mut event| {
                if !normalize_call_source(&mut event, &song_dir) { return None; }
                normalize_paths(&mut event, &song_dir);
                // Keep the same comparison domain on both sides. The raw
                // owned observer also records methods outside CALL_OPERATIONS
                // (for example AFT.GetTexture); native events for those methods
                // are retained in the archive but excluded below.
                if !compared_call_operation(&event) {
                    uncompared.push(event);
                    return None;
                }
                let scene = event["scene"].as_u64().unwrap_or(0);
                normalize_query_handles(&mut event["args"], &mut actual_handles, scene);
                if matches!(event["kind"].as_str(), Some("query" | "modifier")) {
                    normalize_query_handles(&mut event["results"], &mut actual_handles, scene);
                }
                Some(event)
            })
            .collect();
        (actual, capture.dropped, uncompared)
    });
    if dropped != 0 {
        mismatches.push(json!({"field":"dropped_call_trace","deadsync":dropped}));
    }
    if trace["capabilities"]["native_event_scenes"] != true {
        let mut calls = json!({"compared":false,"reason":"native command/query events have no scene epochs"});
        calls["actual"] = Value::Array(actual);
        calls["uncompared_actual"] = Value::Array(uncompared_actual);
        return Ok(calls);
    }
    let mut native_handles = BTreeMap::new();
    let native: Vec<Value> = trace["events"]
        .as_array()
        .ok_or("missing native events")?
        .iter()
        .filter(|event| {
            event
                .pointer("/caller/source")
                .unwrap_or(&event["source"])
                .as_str()
                .is_some_and(|source| source.starts_with("song:/"))
        })
        .filter(|event| compared_call_operation(event))
        .filter(|event| {
            !matches!(
                event["operation"].as_str(),
                Some("Actor.sleep" | "Actor.queuecommand")
            ) || event.get("caller").is_some()
                || event["actor"]
                    .as_str()
                    .is_some_and(|actor| actor.starts_with("def-"))
        })
        .filter(|event| {
            scope == Scope::Full
                || event["scene"].as_u64().is_some_and(|at| {
                    at < scene
                        || at == scene
                            && event["seconds"]
                                .as_f64()
                                .is_some_and(|time| time <= seconds + 1e-6)
                })
        })
        .map(|event| {
            let mut value = event.clone();
            let scene = value["scene"].as_u64().unwrap_or(0);
            normalize_query_handles(&mut value["args"], &mut native_handles, scene);
            if matches!(event["kind"].as_str(), Some("query" | "modifier")) {
                value["results"] = event["detail"]["results"].clone();
                normalize_query_handles(&mut value["results"], &mut native_handles, scene);
            }
            if event["kind"] != "command" {
                if let Some(caller) = event.get("caller") {
                    value["source"] = caller["source"].clone();
                    value["line"] = caller["line"].clone();
                }
            }
            if event["kind"] == "call" {
                value["tween_time_left"] = event["detail"]["native_tween_time_left"].clone();
            }
            value
        })
        .collect();
    if actual.len() != native.len() {
        mismatches.push(
            json!({"field":"command_query_count","native":native.len(),"deadsync":actual.len()}),
        );
    }
    for (native, actual) in native.iter().zip(&actual) {
        if !same_call_event(trace, native, actual)? {
            mismatches
                .push(json!({"field":"command_query_event","native":native,"deadsync":actual}));
        }
    }
    let mut calls = json!({"compared":true,"native_count":native.len(),"actual_count":actual.len()});
    calls["actual"] = Value::Array(actual);
    calls["native"] = Value::Array(native);
    calls["uncompared_actual"] = Value::Array(uncompared_actual);
    Ok(calls)
}

fn compared_call_operation(event: &Value) -> bool {
    event["operation"].as_str().is_some_and(|operation| CALL_OPERATIONS.contains(&operation))
}

fn same_call_event(trace: &Value, native: &Value, actual: &Value) -> Result<bool, String> {
    let keys: &[&str] = match native["kind"].as_str() {
        Some("query") if trace["capabilities"]["native_constructor_queries"] == true =>
            &["scene", "kind", "operation", "source", "line", "phase", "args", "results"],
        Some("query") => &["scene", "kind", "operation", "source", "line", "results"],
        Some("modifier") if trace["capabilities"]["native_modifier_results"] == true =>
            &["scene", "kind", "operation", "source", "line", "phase", "args", "results"],
        Some("modifier") => &["scene", "kind", "operation", "source", "line", "phase", "args"],
        Some("call") => &["scene", "kind", "operation", "source", "line", "args", "tween_time_left"],
        _ => &["scene", "kind", "operation", "source", "line", "command"],
    };
    Ok(keys.iter().all(|key| same_value(&native[*key], &actual[*key]))
        && !(native["kind"] == "call"
            && trace["capabilities"]["native_constructor_queries"] == true
            && native["phase"] != actual["phase"])
        && (number(native, "seconds")? - number(actual, "seconds")?).abs() <= 1e-6
        && (number(native, "beat")? - number(actual, "beat")?).abs() <= 1e-6)
}

fn normalize_call_source(event: &mut Value, song_dir: &str) -> bool {
    let Some(Value::String(source)) = event.get_mut("source") else { return false; };
    if source.contains('\\') {
        *source = source.replace('\\', "/");
    }
    if let Some(start) = source.find(song_dir) {
        source.replace_range(..start + song_dir.len(), "song:/");
        true
    } else {
        source.starts_with("song:/")
    }
}

fn normalize_paths(value: &mut Value, song_dir: &str) {
    match value {
        Value::String(value) => {
            let normalized = if value.contains('\\') {
                std::borrow::Cow::Owned(value.replace('\\', "/"))
            } else { std::borrow::Cow::Borrowed(value.as_str()) };
            if let Some((_, relative)) = normalized.split_once(song_dir) {
                *value = format!("song:/{relative}");
            }
        }
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| normalize_paths(value, song_dir)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| normalize_paths(value, song_dir)),
        _ => {}
    }
}

fn normalize_query_handles(
    value: &mut Value,
    handles: &mut BTreeMap<u64, BTreeMap<String, usize>>,
    scene: u64,
) {
    match value {
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| normalize_query_handles(value, handles, scene)),
        Value::Object(values) => {
            // Historical recorders named native enum-backed objects from the
            // caller's numeric argument. Preserve player and option-level
            // identity while accepting their equivalent native enum names.
            if matches!(values.get("type").and_then(Value::as_str), Some("PlayerState" | "PlayerOptions"))
                && let Some(path) = values.get("path").and_then(Value::as_str).map(str::to_owned)
            {
                let (player, suffix) = path.split_once('/').map_or((path.as_str(), ""), |(p, s)| (p, s));
                let canonical = match player {
                    "player-state:0" => Some("player-state:PlayerNumber_P1"),
                    "player-state:1" => Some("player-state:PlayerNumber_P2"),
                    _ => None,
                };
                if let Some(canonical) = canonical {
                    values.insert("path".into(), json!(if suffix.is_empty() {
                        canonical.to_owned()
                    } else { format!("{canonical}/{suffix}") }));
                }
            }
            // StageStats indexes m_player by the same native PlayerNumber.
            // Historical recorders put the caller's spelling in this path.
            if values.get("type").and_then(Value::as_str) == Some("PlayerStageStats")
                && let Some(path) = values.get("path").and_then(Value::as_str).map(str::to_owned)
                && let Some((stage, player)) = path.rsplit_once("/player:")
                && (stage == "stage" || stage.starts_with("stage:"))
            {
                let canonical = match player {
                    "0" | "PlayerNumber_P1" => Some("P1"),
                    "1" | "PlayerNumber_P2" => Some("P2"),
                    _ => None,
                };
                if let Some(player) = canonical {
                    values.insert("path".into(), json!(format!("{stage}/player:{player}")));
                }
            }
            // Canonicalize allocation IDs by first observation while retaining
            // alias relationships, actor names, and actual named ancestry.
            if values.get("name").is_some_and(Value::is_string)
                && values.get("path").is_some_and(Value::is_string)
                && let Some(id) = values.get("actor").and_then(Value::as_str)
            {
                let id = if let Some(id) = handles.get(&scene).and_then(|handles| handles.get(id)) {
                    *id
                } else {
                    // Allocate an identity string only for its first sighting.
                    // Numbering remains global across scene epochs.
                    let next = handles.values().map(BTreeMap::len).sum::<usize>() + 1;
                    handles.entry(scene).or_default().insert(id.to_owned(), next);
                    next
                };
                values.insert("actor".into(), json!(id));
            }
            values
                .values_mut()
                .for_each(|value| normalize_query_handles(value, handles, scene));
        }
        _ => {}
    }
}

fn same_value(native: &Value, actual: &Value) -> bool {
    match (native, actual) {
        (Value::Number(native), Value::Number(actual)) => native
            .as_f64()
            .zip(actual.as_f64())
            .is_some_and(|(native, actual)| (native - actual).abs() <= 1e-6),
        (Value::Array(native), Value::Array(actual)) => {
            native.len() == actual.len()
                && native
                    .iter()
                    .zip(actual)
                    .all(|(native, actual)| same_value(native, actual))
        }
        _ => native == actual,
    }
}

fn coverage(trace: &Value, scope: Scope, calls: &Value, replayed: usize, total: usize) -> Value {
    let observed: BTreeSet<&str> = trace["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|event| {
            event
                .pointer("/caller/source")
                .unwrap_or(&event["source"])
                .as_str()
                .is_some_and(|source| source.starts_with("song:/"))
        })
        .filter_map(|event| event["operation"].as_str())
        .collect();
    let compared: BTreeSet<_> = CALL_OPERATIONS
        .iter()
        .copied()
        .chain([
            "ScreenManager.SetNewScreen",
            "ScreenWithMenuElements.StartTransitioningScreen",
            "MessageManager.Broadcast",
        ])
        .collect();
    let theme: BTreeSet<&str> = trace["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|event| {
            event["source"]
                .as_str()
                .is_some_and(|source| source.starts_with("song:/"))
                && event
                    .pointer("/caller/source")
                    .and_then(Value::as_str)
                    .is_some_and(|source| !source.starts_with("song:/"))
        })
        .filter_map(|event| event["operation"].as_str())
        .collect();
    let mut pending = Vec::new();
    if trace["events"].as_array().is_some_and(|events| events.iter().any(|event|
        (event.pointer("/caller/source").is_some_and(|source| source == "=(tail call)")
            || event.pointer("/caller/tail_call").and_then(Value::as_bool) == Some(true))
        && event["source"].as_str().is_some_and(|source| source.starts_with("song:/"))))
    {
        pending.push("native tail-call provenance is unavailable".to_owned());
    }
    if trace["capabilities"]["native_modifier_results"] != true
        && trace["events"].as_array().is_some_and(|events| events.iter().any(|event|
            event["kind"] == "modifier" && event.pointer("/caller/source").unwrap_or(&event["source"])
                .as_str().is_some_and(|source| source.starts_with("song:/"))))
    {
        pending.push("native modifier return values are not captured".to_owned());
    }
    if scope != Scope::Full || replayed != total {
        pending.push("complete song frame replay".to_owned());
    }
    if calls["compared"] != true {
        pending.push("native command/query scene identities".to_owned());
    }
    if trace["capabilities"]["native_call_sources"] != true {
        pending.push("native method events lack immediate caller identity".to_owned());
    }
    if trace["capabilities"]["native_constructor_queries"] != true {
        pending.push("native constructor query results are not captured".to_owned());
    }
    let uncovered: Vec<_> = observed
        .iter()
        .filter(|operation| !compared.contains(*operation))
        .copied()
        .collect();
    if !uncovered.is_empty() {
        pending.push(format!(
            "uncompared song operations: {}",
            uncovered.join(", ")
        ));
    }
    let drawables: BTreeSet<_> = trace["actor_definitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|definition| definition["class"].as_str())
        .filter(|class| !matches!(*class, "Actor" | "ActorFrame"))
        .collect();
    if trace["actor_definitions"]
        .as_array()
        .is_none_or(Vec::is_empty)
    {
        pending.push("native actor definitions are missing".to_owned());
    } else if !drawables.is_empty() {
        pending.push(format!(
            "native drawable state/resources: {}",
            drawables.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    json!({"scope":"captured-song-lua-context", "observed_song_operations":observed,
        "uncompared_song_operations":uncovered, "nested_theme_operations":theme, "pending":pending,
        "global_pending":["full theme stage commitment and history", "unqueried score/alive-time observer fields",
            "additional game/style/input/course contexts", "production live mode activation"]})
}

#[cfg(test)]
mod tests {
    #[test]
    fn background_cache_reuses_owners_and_replays_focus_schedule() {
        paths::init();
        super::start_call_log(100_000).unwrap();
        let dir = std::env::temp_dir().join(format!("deadsync-live-background-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test directory");
        for name in ["A", "B"] {
            std::fs::write(dir.join(format!("{name}.lua")), format!(r#"
BG_EVENTS = BG_EVENTS or {{}}
BG_INSTANCES = BG_INSTANCES or {{}}
local function note(event) MESSAGEMAN:Broadcast('{name}' .. event) end
return Def.ActorFrame {{
    InitCommand = function(self)
        table.insert(BG_INSTANCES, self)
        note('Init')
        self.activations = 0
        self:x(99)
    end,
    OnCommand = function(self)
        assert(#BG_INSTANCES == 2)
        assert(self == BG_INSTANCES['{name}' == 'A' and 1 or 2])
        note('On' .. tostring(self:GetX()))
        self.activations = self.activations + 1
        self:x(self.activations * 10)
        self:SetUpdateFunction(function(self, delta) note('Update' .. tostring(delta)) end)
    end,
    GainFocusCommand = function(self) note('Gain' .. tostring(self.activations)) end,
    LoseFocusCommand = function(self) note('Lose'); self:sleep(0.25):queuecommand('Faded') end,
    FadedCommand = function(self) note('Faded') end,
}}
"#)).expect("background script");
        }
        let simfile = dir.join("cache.sm");
        std::fs::write(&simfile, b"#TITLE:Background cache;#BPMS:0=120;#BGCHANGES:0=A.lua=1=0=0=1,2=B.lua=1=0=0=1,4=A.lua=2=0=0=1,6=A.lua=0.5=0=0=1;#NOTES:dance-single:Control:Hard:1:0,0,0,0,0:\n1000\n0000\n0000\n0000\n;").expect("simfile");
        let song = deadsync_simfile::song::parse_song_meta_file(&simfile,
            &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
            0.0, |_| 0.0).expect("song metadata");
        let mut context = deadsync_song_lua::SongLuaCompileContext::new(&dir, "Background cache");
        context.live = true;
        context.defer_initial_live_update = true;
        context.song_display_bpms = [120.0; 2];
        context.song_timing_bpms = vec![(0.0, 120.0)];
        let mut prepared = deadsync_song_lua::playback::prepare_song_lua(&song, &context,
            deadsync_assets::song_lua::compile_song_lua, deadsync_assets::song_lua::compile_song_lua_layers);
        let mut scene = prepared.take_live_scene().expect("one shared VM").expect("owned scene");
        let startup = scene.take_startup_output();
        assert!(startup.errors.is_empty());
        let initialized = startup.broadcasts.iter().filter(|(_, name, _)| name == "AInit" || name == "BInit").count();
        assert_eq!(initialized, 2, "native background cache retains one actor per definition");
        assert!(startup.broadcasts.iter().all(|(_, name, _)| !name.contains("On") && !name.contains("Gain")), "backgrounds stay dormant during construction");
        for (seconds, expected) in [
            (0.125, vec!["AOn0", "AGain1", "AUpdate0.125"]),
            (0.25, vec!["AUpdate0.125"]),
            (1.125, vec!["ALose", "BOn0", "BGain1", "BUpdate0.125", "AUpdate0.125"]),
            (1.25, vec!["BUpdate0.125", "AUpdate0.125"]),
            (1.375, vec!["BUpdate0.125"]),
            (1.5, vec!["BUpdate0.125"]),
            (2.125, vec!["BLose", "AOn0", "AGain2", "AUpdate0.25", "BUpdate0.125"]),
            (3.125, vec!["AOn0", "AGain3", "AUpdate0.0625"]),
        ] {
            let output = scene.advance(deadsync_song_lua::SongLuaLiveFrame { music_seconds: seconds, beat: seconds * 2.0,
                screen_seconds: seconds + 4.0, delta_seconds: 0.125, ..Default::default() }).expect("native background frame");
            assert!(output.errors.is_empty(), "{:#?}", output.errors);
            let actual = output.broadcasts.iter().map(|(_, name, _)| name.as_str()).collect::<Vec<_>>();
            assert_eq!(actual, expected, "music second {seconds}");
        }
        let song = deadsync_simfile::song::parse_song_meta_file(&simfile,
            &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
            0.0, |_| 0.0).unwrap();
        assert_eq!(song.background_lua_changes.len(), 2, "metadata preserves cached owner cardinality");
        let calls = super::CALL_EVENTS.with_borrow(|capture| capture.events.clone());
        assert!(calls.iter().any(|event| event["operation"] == "Actor.x" && event["phase"] == "callback"),
            "authored calls inside a native merged background command remain callback observations");
        assert!(!calls.iter().any(|event| event["kind"] == "command" && event["command"] == "OnCommand"
            && event["source"].as_str().is_some_and(|source| source.contains(dir.to_str().unwrap()))),
            "UpperLeft merges root On with the native effect; its command identity belongs to ActorDef, not the song");
    }

    use super::{gameplay, paths, GameplayAudioSnapshot, GameplayConfig, GameplaySession,
        GameplayStreamClockSnapshot, GameplayViewport, Arc, json};

    #[test]
    fn call_comparison_uses_the_same_method_domain_without_losing_value_checks() {
        use serde_json::json;
        let outside = json!({"scene":1,"kind":"query","operation":"ActorFrameTexture.GetTexture",
            "source":"song:/control.lua","line":10,"phase":"callback","seconds":0.0,"beat":0.0,
            "args":[],"results":[{"type":"RageTexture"}]});
        let inside = json!({"scene":1,"kind":"query","operation":"Sprite.GetTexture",
            "source":"song:/control.lua","line":10,"phase":"callback","seconds":0.0,"beat":0.0,
            "args":[],"results":[{"type":"nil"}]});
        let mut native_outside = outside.clone();
        native_outside["detail"] = json!({"results":outside["results"]});
        let mut native_inside = inside.clone();
        native_inside["detail"] = json!({"results":inside["results"]});
        let trace = json!({"capabilities":{"native_event_scenes":true,"native_constructor_queries":true},
            "events":[native_outside,native_inside]});
        for wrong_value in [false, true] {
            super::start_call_log(4).unwrap();
            super::record_call_event(outside.clone());
            let mut observed = inside.clone();
            if wrong_value { observed["results"] = json!([{"type":"RageTexture"}]); }
            super::record_call_event(observed);
            let mut mismatches = Vec::new();
            let report = super::compare_calls(&trace, std::path::Path::new("control/control.ssc"),
                1, 0.0, super::Scope::Full, &mut mismatches).unwrap();
            assert_eq!(report["native_count"], 1);
            assert_eq!(report["actual_count"], 1);
            assert_eq!(report["uncompared_actual"], json!([outside]));
            assert_eq!(mismatches.len(), usize::from(wrong_value), "compared getter values still fail");
            if wrong_value { assert_eq!(mismatches[0]["field"], "command_query_event"); }
        }
    }

    #[test]
    fn call_trace_capacity_keeps_overflow_visible() {
        use log::Log;
        super::start_call_log(2).expect("local call trace");
        let record = log::Record::builder().args(format_args!("{{}}"))
            .target("deadsync_song_lua::trace").level(log::Level::Trace).build();
        for _ in 0..3 { super::CALL_LOG.log(&record); }
        super::CALL_EVENTS.with_borrow(|capture| {
            assert_eq!(capture.events.len(), 2);
            assert_eq!(capture.dropped, 1);
        });
        assert!(super::start_call_log(0).is_err());
    }

    #[test]
    fn concurrent_call_logs_remain_separate() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (1..=2).map(|owner| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                super::start_call_log(3).expect("worker call trace");
                barrier.wait();
                for _ in 0..owner + 2 {
                    log::trace!(target: "deadsync_song_lua::trace", "{{\"owner\":{owner}}}");
                }
                barrier.wait();
                super::CALL_EVENTS.with_borrow(|capture| {
                    assert_eq!(capture.events.len(), 3);
                    assert_eq!(capture.dropped, owner - 1);
                    assert!(capture.events.iter().all(|event| event["owner"] == owner));
                });
            })
        }).collect();
        for handle in handles { handle.join().expect("isolated worker observations"); }
    }

    #[test]
    fn option_trace_keeps_native_returns_and_strings() {
        use deadsync_song_lua::{SongLuaCompileContext, SongLuaNoteskinResolver,
            compile_song_lua_with_default_host};
        use std::{fs, sync::Arc};
        super::start_call_log(2_000_000).expect("install option trace");
        let dir = std::env::temp_dir().join(format!("deadsync-option-call-trace-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("option trace directory");
        let entry = dir.join("options.lua");
        fs::write(&entry, r#"
local options = GAMESTATE:GetSongOptionsObject('ModsLevel_Song')
local rate, speed = options:MusicRate()
assert(rate == 1 and speed == 1)
local before, before_speed = options:MusicRate(1.25, 2)
assert(before == 1 and before_speed == 1)
rate, speed = options:MusicRate()
assert(rate == 1.25 and speed == 2)
local player = GAMESTATE:GetPlayerState('PlayerNumber_P1')
assert(select('#', player:SetPlayerOptions('ModsLevel_Song', '*3 75% Reverse')) == 0)
local text = player:GetPlayerOptionsString('ModsLevel_Song')
assert(text:find('75%% Reverse'))
return Def.ActorFrame{}
"#).expect("write real option calls");
        let mut context = SongLuaCompileContext::new(&dir, "option trace control");
        context.live = true;
        let compiled = compile_song_lua_with_default_host::<(), (), _>(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None, |_, _| None).expect("compile real option operations");
        assert!(compiled.live_scene.is_some());
        let events = super::CALL_EVENTS.with_borrow(|capture| capture.events.clone());
        assert_eq!(events.iter().map(|e| e["operation"].as_str().expect("operation")).collect::<Vec<_>>(),
            ["GameState.GetSongOptionsObject", "SongOptions.MusicRate", "SongOptions.MusicRate",
             "SongOptions.MusicRate", "GameState.GetPlayerState", "PlayerState.SetPlayerOptions",
             "PlayerState.GetPlayerOptionsString"]);
        assert_eq!(events[0]["results"], serde_json::json!([{"type":"SongOptions"}]));
        assert_eq!(events[1]["kind"], "query");
        assert_eq!(events[2]["kind"], "modifier");
        assert_eq!(events[2]["args"], serde_json::json!([1.25, 2]));
        assert_eq!(events[2]["results"], serde_json::json!([1, 1]));
        assert_eq!(events[3]["results"], serde_json::json!([1.25, 2]));
        assert_eq!(events[5]["kind"], "modifier");
        assert_eq!(events[5]["args"], serde_json::json!(["ModsLevel_Song", "*3 75% Reverse"]));
        assert_eq!(events[5]["results"], serde_json::json!([]));
        assert!(events[6]["results"][0].as_str().expect("option string").contains("75% Reverse"));
        assert!(events.iter().all(|e| e["phase"] == "construction" && e["line"] == 0));
    }

    #[test]
    fn modifier_comparison_keeps_values_speeds_returns_and_clocks() {
        use serde_json::json;
        let trace = json!({"capabilities":{"native_modifier_results":true}});
        let native = json!({"scene":1,"kind":"modifier","operation":"PlayerOptions.Boost",
            "source":"song:/mods.lua","line":12,"phase":"callback",
            "args":[0.5,10000],"results":[0.25,1],"seconds":2.0,"beat":4.0});
        assert!(super::same_call_event(&trace, &native, &native).expect("complete observation"));
        for (pointer, value) in [
            ("/args/0", json!(0.75)), ("/args/1", json!(1)),
            ("/results/0", json!(0.5)), ("/results/1", json!(10000)),
            ("/results", json!(null)), ("/kind", json!("call")),
            ("/phase", json!("construction")), ("/seconds", json!(2.01)),
            ("/beat", json!(4.01)),
        ] {
            let mut wrong = native.clone();
            *wrong.pointer_mut(pointer).expect("control field") = value;
            assert!(!super::same_call_event(&trace, &native, &wrong).expect("control clock"), "accepted {pointer}");
        }
        let legacy = json!({"events":[{"kind":"modifier","operation":"PlayerOptions.Boost",
            "caller":{"source":"song:/mods.lua"}}], "actor_definitions":[{"class":"ActorFrame"}]});
        let coverage = super::coverage(&legacy, super::Scope::Full, &json!({"compared":true}), 1, 1);
        assert!(coverage["pending"].as_array().expect("pending coverage")
            .iter().any(|item| item == "native modifier return values are not captured"));
        let tail = json!({"events":[{"kind":"query","operation":"ActorFrame.GetChild",
            "source":"song:/mods.lua","caller":{"source":"=(tail call)","line":-1}}],
            "actor_definitions":[{"class":"ActorFrame"}]});
        let coverage = super::coverage(&tail, super::Scope::Full, &json!({"compared":true}), 1, 1);
        assert!(coverage["pending"].as_array().expect("pending coverage")
            .iter().any(|item| item == "native tail-call provenance is unavailable"));
    }

    #[test]
    fn player_number_paths_keep_players_and_option_levels_distinct() {
        use serde_json::json;
        use std::collections::BTreeMap;
        let mut numeric = json!([{"type":"PlayerState","path":"player-state:0"},
            {"type":"PlayerOptions","path":"player-state:0/options:ModsLevel_Song"}]);
        let named = json!([{"type":"PlayerState","path":"player-state:PlayerNumber_P1"},
            {"type":"PlayerOptions","path":"player-state:PlayerNumber_P1/options:ModsLevel_Song"}]);
        super::normalize_query_handles(&mut numeric, &mut BTreeMap::new(), 1);
        assert!(super::same_value(&numeric, &named));
        for path in ["player-state:PlayerNumber_P2/options:ModsLevel_Song",
                     "player-state:PlayerNumber_P1/options:ModsLevel_Current"] {
            let mut wrong = named.clone();
            wrong[1]["path"] = json!(path);
            assert!(!super::same_value(&numeric, &wrong), "accepted {path}");
        }
    }

    #[test]
    fn player_stage_stats_paths_preserve_player_stage_and_literal_identity() {
        use serde_json::json;
        use std::collections::BTreeMap;
        let mut numeric = json!([
            {"type":"PlayerStageStats","path":"stage:current/player:0"},
            {"type":"PlayerStageStats","path":"stage:current/player:1"},
            {"type":"PlayerStageStats","path":"stage:played:0/player:0"},
        ]);
        let named = json!([
            {"type":"PlayerStageStats","path":"stage:current/player:P1"},
            {"type":"PlayerStageStats","path":"stage:current/player:P2"},
            {"type":"PlayerStageStats","path":"stage:played:0/player:P1"},
        ]);
        super::normalize_query_handles(&mut numeric, &mut BTreeMap::new(), 1);
        assert_eq!(numeric, named, "native numeric enum aliases address the same player stats");
        for path in ["stage:current/player:P2", "stage:played:0/player:P1", "stage:played:1/player:P1"] {
            let mut wrong = named.clone();
            wrong[0]["path"] = json!(path);
            assert!(!super::same_value(&numeric, &wrong), "accepted different identity {path}");
        }
        let mut literals = json!([
            "stage:current/player:0",
            {"type":"PlayerOptions","path":"stage:current/player:0"},
            {"type":"PlayerStageStats","path":"stage:current/player:2"},
            {"type":"PlayerStageStats","path":"other:0/player:0"},
        ]);
        let unchanged = literals.clone();
        super::normalize_query_handles(&mut literals, &mut BTreeMap::new(), 1);
        assert_eq!(literals, unchanged);
    }

    #[test]
    fn native_controller_inputs_reject_cpu_and_partial_autoplay() {
        for (mode, enabled) in [(0, false), (1, true)] {
            let trace = serde_json::json!({"input": {"autoplay": mode}});
            assert_eq!(super::native_autoplay(&trace).expect("recorded controller"), enabled);
        }
        for mode in [serde_json::json!(2), serde_json::json!(0.5), serde_json::json!(-1),
                     serde_json::json!(null), serde_json::json!("1")] {
            let trace = serde_json::json!({"input": {"autoplay": mode}});
            assert!(super::native_autoplay(&trace).is_err(), "accepted {mode}");
        }
    }

    #[test]
    fn native_default_profile_uses_fresh_bootstrap_inputs() {
        let mut trace = serde_json::json!({"input": {"gameplay_preferences": {"default_modifiers": ""}}});
        let profiles = super::native_profile_inputs(&trace).expect("fresh native defaults");
        assert!(profiles.iter().all(|profile| profile.scroll_speed == deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0)));
        trace["input"]["gameplay_preferences"]["default_modifiers"] = serde_json::json!("C600");
        assert!(super::native_profile_inputs(&trace).is_err());
    }



    #[test]
    fn native_danger_visibility_uses_initial_profile_input() {
        let mut trace=serde_json::json!({"input":{"gameplay_preferences":{"default_modifiers":"",
            "players":[{"theme_modifiers":{"HideDanger":true}},
                {"theme_modifiers":{"HideDanger":false}}]}}});
        let profiles=super::native_profile_inputs(&trace).unwrap();
        assert!(profiles[0].hide_danger && !profiles[1].hide_danger);
        trace["input"]["gameplay_preferences"]["players"][0]["theme_modifiers"]["HideDanger"]=serde_json::json!("true");
        assert!(super::native_profile_inputs(&trace).is_err());
    }

    #[test]
    fn native_theme_timing_windows_use_initial_inputs_without_query_calibration() {
        let mut trace=serde_json::json!({"input":{"gameplay_preferences":{"players":[
            {"theme_modifiers":{"TimingWindows":[true,true,true,true,true]}},
            {"theme_modifiers":{"TimingWindows":[false,true,false,true,false]}}
        ]}},"events":[{"operation":"ActorFrame.GetNumChildren","results":[999]}]});
        assert_eq!(super::native_theme_timing_windows(&trace).unwrap(),
            [[true;5],[false,true,false,true,false]]);
        trace["events"][0]["results"]=serde_json::json!([1]);
        assert_eq!(super::native_theme_timing_windows(&trace).unwrap(),
            [[true;5],[false,true,false,true,false]]);
        for invalid in [serde_json::json!([true,true]),serde_json::json!([true,true,true,true,1]),serde_json::json!("all")] {
            trace["input"]["gameplay_preferences"]["players"][0]["theme_modifiers"]["TimingWindows"]=invalid;
            assert!(super::native_theme_timing_windows(&trace).is_err());
        }
        assert_eq!(super::native_theme_timing_windows(&serde_json::json!({})).unwrap(),[[true;5];2]);
    }

    #[test]
    fn native_step_statistics_uses_initial_selection_only() {
        let mut trace = serde_json::json!({"input":{"gameplay_preferences":{"default_modifiers":"",
            "players":[{"theme_modifiers":{"DataVisualizations":"None"}},
                {"theme_modifiers":{"DataVisualizations":"Step Statistics"}}]}}});
        let profiles = super::native_profile_inputs(&trace).unwrap();
        assert!(profiles[0].step_statistics.is_empty());
        assert!(!profiles[1].step_statistics.is_empty());
        trace["input"]["gameplay_preferences"]["players"][0]["theme_modifiers"]["DataVisualizations"] = serde_json::json!(false);
        assert!(super::native_profile_inputs(&trace).is_err());
    }

    #[test]
    fn native_bpm_layout_uses_initial_upper_nps_option() {
        let mut trace = serde_json::json!({"input":{"gameplay_preferences":{"default_modifiers":"",
            "players":[{"theme_modifiers":{"NPSGraphAtTop":true}}, {"theme_modifiers":{"NPSGraphAtTop":false}}]}}});
        let profiles = super::native_profile_inputs(&trace).unwrap();
        assert!(profiles[0].nps_graph_at_top && !profiles[1].nps_graph_at_top);
        trace["input"]["gameplay_preferences"]["players"][0]["theme_modifiers"]["NPSGraphAtTop"] = serde_json::json!(1);
        assert!(super::native_profile_inputs(&trace).is_err());
    }

    #[test]
    fn autonomous_theme_queries_stay_outside_song_trace_but_nested_calls_remain() {
        use deadsync_song_lua::{SongLuaCompileContext, SongLuaNoteskinResolver, compile_song_lua_with_default_host};
        use std::{fs, sync::Arc};
        super::start_call_log(2_000_000).unwrap();
        let dir=std::env::temp_dir().join(format!("deadsync-theme-call-scope-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();let entry=dir.join("scope.lua");
        fs::write(&entry,r#"
local nested=assert(loadstring("return function() return GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate() end", '@theme:/nested-rate-control.lua'))()
return Def.ActorFrame { OnCommand=function(self) assert(nested()==1) end }
"#).unwrap();
        let mut context=SongLuaCompileContext::new(&dir,"theme call scope");context.live=true;
        let compiled=compile_song_lua_with_default_host::<(),(),_>(&entry,&context,SongLuaNoteskinResolver::default(),
            |_,_,_|Ok(Arc::<[()]>::from([])), |_|None, |_,_|None).unwrap();
        assert!(compiled.live_scene.is_some());
        let events=super::CALL_EVENTS.with_borrow(|capture|capture.events.clone());
        let nested=events.iter().filter(|event|event["source"]=="@theme:/nested-rate-control.lua").collect::<Vec<_>>();
        assert_eq!(nested.len(),2,"song-triggered nested theme calls remain observed");
        assert!(nested.iter().all(|event|event["phase"]=="callback"));
        assert!(!events.iter().any(|event|event["source"].as_str().is_some_and(|source|
            source.contains("Shared/BPMDisplay.lua"))),"autonomous native theme updater has no song command context");
    }

    #[test]
    fn native_life_meter_uses_initial_theme_options_only() {
        let mut trace = serde_json::json!({"input":{"gameplay_preferences":{"default_modifiers":"",
            "players":[{"theme_modifiers":{"HideLifebar":true,"LifeMeterType":"Vertical"}},
                       {"theme_modifiers":{"HideLifebar":false,"LifeMeterType":"Standard"}}]}}});
        let profiles = super::native_profile_inputs(&trace).unwrap();
        assert!(profiles[0].hide_lifebar && !profiles[1].hide_lifebar);
        assert_eq!(profiles[0].lifemeter_type, deadsync_profile::LifeMeterType::Vertical);
        assert_eq!(profiles[1].lifemeter_type, deadsync_profile::LifeMeterType::Standard);
        trace["input"]["gameplay_preferences"]["players"][0]["theme_modifiers"]["LifeMeterType"] = serde_json::json!("unknown");
        assert!(super::native_profile_inputs(&trace).is_err());
        trace["input"]["gameplay_preferences"]["players"][0]["theme_modifiers"]["LifeMeterType"] = serde_json::json!("Standard");
        trace["input"]["gameplay_preferences"]["players"][0]["theme_modifiers"]["HideLifebar"] = serde_json::json!("true");
        assert!(super::native_profile_inputs(&trace).is_err());
    }

    #[test]
    fn native_background_filter_uses_initial_theme_options_only() {
        let mut trace = serde_json::json!({"input":{"gameplay_preferences":{
            "default_modifiers":"", "players":[
                {"theme_modifiers":{"BackgroundFilter":"Darker"}},
                {"theme_modifiers":{"BackgroundFilter":"Off"}}
            ]}}});
        let profiles = super::native_profile_inputs(&trace).unwrap();
        assert_eq!(profiles[0].background_filter.percent(), 75);
        assert_eq!(profiles[1].background_filter.percent(), 0);
        trace["input"]["gameplay_preferences"]["players"][0]["theme_modifiers"]["BackgroundFilter"] = serde_json::json!("unknown");
        assert!(super::native_profile_inputs(&trace).is_err());
    }

    #[test]
    fn native_reload_starts_with_the_requested_style() {
        use deadsync_gameplay::GameplayInputPlayStyle;
        let single = super::native_gameplay_session(&json!({
            "style":"double", "input":{"style":"single"}
        })).unwrap();
        assert_eq!(single.play_style, GameplayInputPlayStyle::Single);
        assert_eq!(single.joined_sides, [true, false]);
        let versus = super::native_gameplay_session(&json!({
            "style":"double", "input":{"style":"versus"}
        })).unwrap();
        assert_eq!(versus.play_style, GameplayInputPlayStyle::Versus);
        assert_eq!(versus.joined_sides, [true, true]);
    }

    #[test]
    fn native_double_session_selects_double_steps_and_runs_eight_column_scene() {
        paths::init();
        let dir = std::env::temp_dir().join(format!("deadsync-double-native-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let simfile = dir.join("mixed.sm");
        std::fs::write(&simfile, "#TITLE:Double session;#BPMS:0=120;#FGCHANGES:-24=default.lua=1=0=0=1;\n#NOTES:dance-single:Single:Challenge:1:0,0,0,0,0:\n0001\n0000\n0000\n0000\n;\n#NOTES:dance-double:Double:Challenge:1:0,0,0,0,0:\n00000001\n00000000\n00000000\n00000000\n;").unwrap();
        std::fs::write(dir.join("default.lua"), r#"
return Def.ActorFrame { OnCommand=function(self)
    local style = GAMESTATE:GetCurrentStyle()
    assert(style:GetName() == 'double' and style:ColumnsPerPlayer() == 8)
    assert(GAMESTATE:GetCurrentSteps(PLAYER_1):GetStepsType() == 'StepsType_Dance_Double')
    local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
    local columns = player:GetChild('NoteField'):GetColumnActors()
    assert(#columns == 8 and columns[8] ~= nil)
    local spline = columns[8]:GetZoomHandler():GetSpline()
    spline:SetSize(1); spline:SetPoint(1, {2,2,2}); spline:Solve()
    local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Preferred')
    local mapping = GetColumnMapping(PLAYER_1)
    assert(#mapping == 8 and mapping[8] == 8)
    options:Mirror(true)
    mapping = GetColumnMapping(PLAYER_1)
    for column=1,8 do assert(mapping[column] == 9-column) end
    options:Mirror(false); options:NoHolds(true)
    assert(GetColumnMapping(PLAYER_1) == nil, 'unsupported cue transform declines mapping')
    options:NoHolds(false)
    MESSAGEMAN:Broadcast('DoubleSceneReady')
end }
"#).unwrap();
        let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, -0.008).unwrap());
        let session = super::native_gameplay_session(&json!({"style":"double"})).unwrap();
        let selected = super::native_starting_chart(&song, &session, Some("Challenge")).unwrap();
        assert_eq!(selected, 1, "same-difficulty single chart is not the double chart");
        assert!(super::native_starting_chart(&song, &session, Some("Hard")).is_err());
        let chart = Arc::new(song.charts[selected].clone());
        let charts = [chart.clone(), chart];
        let payload = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[selected], -0.008).unwrap().remove(0));
        let payloads = [payload.clone(), payload];
        let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
        let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
        let config = GameplayConfig { global_offset_seconds: -0.008, ..Default::default() };
        let viewport = GameplayViewport::default();
        let mut context = deadsync_shell::gameplay_entry::song_lua_context(&song, &charts,
            std::array::from_fn(|p| &payloads[p].timing), &profiles, &scroll, 1.0, viewport,
            (854, 480), 16.0/9.0, 1.0, &session, &config, "null", 1, true).unwrap();
        context.defer_initial_live_update = true;
        let prepared = deadsync_song_lua::playback::prepare_song_lua(&song, &context,
            deadsync_assets::song_lua::compile_song_lua, deadsync_assets::song_lua::compile_song_lua_layers);
        let mut state = gameplay::init(song, charts, payloads, viewport, session, config,
            5, 1.0, scroll, profiles, None, None, None, Arc::from("EVENT"), None, None, None,
            None, None, [deadsync_gameplay::CourseLifeConfig::Bar; 2], false, None, None,
            [0; 2], prepared, Default::default());
        assert_eq!(state.gameplay.cols_per_player(), 8);
        let output = state.song_lua.as_mut().expect("retained double scene")
            .advance(deadsync_song_lua::SongLuaLiveFrame { music_seconds: 0.0,
                screen_seconds: 0.1, delta_seconds: 0.1, ..Default::default() }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!(output.broadcasts.iter().any(|(_, name, _)| name == "DoubleSceneReady"), "double On executed");
    }

    #[test]
    fn gameplay_reload_rebuilds_a_single_session_as_double() {
        use deadsync_song_lua::{SongLuaLiveFrame, SongLuaLiveRequest};
        paths::init();
        for extension in ["sm", "ssc"] {
        let dir = std::env::temp_dir().join(format!("deadsync-single-double-reload-{extension}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let simfile = dir.join(format!("mixed.{extension}"));
        let mut source = "#VERSION:0.83;#TITLE:Style reload;#BPMS:0=120;#FGCHANGES:0=default.lua=1=0=0=1;\n".to_owned();
        for (kind, description, notes) in [
            ("dance-single", "Single", "0001\n0000\n0000\n0000"),
            ("dance-couple", "Couple", "00000001\n00000000\n00000000\n00000000"),
            ("dance-routine", "Routine", "00000001\n00000000\n00000000\n00000000"),
            ("dance-double", "Double", "00000001\n00000000\n00000000\n00000000"),
        ] {
            if extension == "sm" {
                source.push_str(&format!("#NOTES:{kind}:{description}:Challenge:1:0,0,0,0,0:\n{notes}\n;\n"));
            } else {
                source.push_str(&format!("#NOTEDATA:;#STEPSTYPE:{kind};#DESCRIPTION:{description};#CHARTNAME:{description};#CREDIT:Author;#DIFFICULTY:Challenge;#METER:1;#NOTES:\n{notes}\n;\n"));
            }
        }
        std::fs::write(&simfile, source).unwrap();
        std::fs::write(dir.join("default.lua"), r#"
return Def.ActorFrame { OnCommand=function(self)
    self:sleep(9999)
    local all = GAMESTATE:GetCurrentSong():GetAllSteps()
    assert(#all == 4)
    assert(all[2]:GetStepsType() == 'StepsType_Dance_Couple')
    assert(all[3]:GetStepsType() == 'StepsType_Dance_Routine')
    assert(all[4] == GAMESTATE:GetCurrentSong():GetStepsByStepsType('StepsType_Dance_Double')[1])
    if GAMESTATE:GetCurrentStyle():GetName() == 'single' then
        GAMESTATE:SetCurrentStyle('double')
        GAMESTATE:SetCurrentSteps(PLAYER_1,
            GAMESTATE:GetCurrentSong():GetStepsByStepsType('StepsType_Dance_Double')[1])
        SCREENMAN:SetNewScreen('ScreenGameplay')
    else
        assert(#SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors() == 8)
        MESSAGEMAN:Broadcast('ReloadedDoubleReady')
    end
end }
"#).unwrap();
        let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, -0.008).unwrap());
        let mut session = GameplaySession::default();
        let mut charts = std::array::from_fn(|_| Arc::new(song.charts[0].clone()));
        let first = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], -0.008).unwrap().remove(0));
        let mut payloads = [first.clone(), first];
        let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
        let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
        let viewport = GameplayViewport::default();
        let config = GameplayConfig { global_offset_seconds: -0.008, ..Default::default() };
        let context = deadsync_shell::gameplay_entry::song_lua_context(&song, &charts,
            std::array::from_fn(|p| &payloads[p].timing), &profiles, &scroll, 1.0, viewport,
            (854,480), 16.0/9.0, 0.0, &session, &config, "null", 1, true).unwrap();
        assert_eq!(song.charts.len(), 2, "couple and routine remain outside playable selection");
        assert_eq!(context.song_step_chart_indices, vec![Some(0), None, None, Some(1)]);
        let mut prepared = deadsync_song_lua::playback::prepare_song_lua(&song, &context,
            deadsync_assets::song_lua::compile_song_lua, deadsync_assets::song_lua::compile_song_lua_layers);
        let mut scene = prepared.take_live_scene().unwrap().unwrap();
        let frame = SongLuaLiveFrame { music_seconds: 0.1, screen_seconds: 0.1,
            delta_seconds: 0.1, ..Default::default() };
        let output = scene.advance(frame.clone()).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(output.request, Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into())));
        let prepared = deadsync_song_lua::playback::reload_song_lua(&song, scene,
            deadsync_assets::song_lua::reload_song_lua).unwrap();
        assert_eq!(prepared.loaded_style(), Some("double"));
        assert_eq!(prepared.loaded_steps().unwrap()[0], Some(1), "native catalog selection maps to its playable payload");
        deadsync_shell::gameplay_entry::apply_song_lua_reload(&song, &mut charts,
            &mut payloads, &prepared, &mut session, -0.008).unwrap();
        assert_eq!(session.play_style, deadsync_gameplay::GameplayInputPlayStyle::Double);
        assert_eq!(session.joined_sides, [true,false]);
        assert_eq!(charts[0].chart_type, "dance-double");
        let mut state = gameplay::init(song, charts, payloads, viewport, session, config,
            5, 1.0, scroll, profiles, None, None, None, Arc::from("EVENT"), None, None, None,
            None, None, [deadsync_gameplay::CourseLifeConfig::Bar; 2], false, None, None,
            [0; 2], prepared, Default::default());
        assert_eq!(state.gameplay.cols_per_player(), 8);
        let output = state.song_lua.as_mut().unwrap().advance(frame).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!(output.source_broadcasts.iter().any(|event| event.name == "ReloadedDoubleReady"));
        }
    }

    #[test]
    fn native_sound_length_exposes_whole_milliseconds() {
        let dir = std::env::temp_dir().join(format!("deadsync-judgment-sound-millis-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let frames = 12_345_u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF"); wav.extend_from_slice(&(36 + frames * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt "); wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes()); wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes()); wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes()); wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data"); wav.extend_from_slice(&(frames * 2).to_le_bytes());
        wav.resize(wav.len() + frames as usize * 2, 0);
        std::fs::write(dir.join("fractional.wav"), wav).unwrap();
        let source = format!(r#"
return Def.Sound {{ File=[=[{}]=], OnCommand=function(self)
    local length = self:get():get_length()
    assert(math.abs(length - 1.543) < 0.0000001,
        'RageSound returns whole milliseconds: length='..tostring(length)..' File='..tostring(self.File))
    assert(self:get():get_length() == length, 'cached length preserves the same native value')
    MESSAGEMAN:Broadcast('NativeSoundMillis')
end }}
"#, dir.join("fractional.wav").display());
        let mut state = owned_judgment_control("sound-millis", "0000\n0000\n0000\n0000\n", &source, 0.0);
        let output = update_judgment_control(&mut state, -250_000_000, 1_000_000_000);
        assert!(output.source_broadcasts.iter().any(|event| event.name == "NativeSoundMillis"));
    }

    #[test]
    fn native_capture_rejects_incomplete_or_failed_traces() {
        let complete = serde_json::json!({
            "oracle": "itgmania_song_lua_semantic_trace",
            "capabilities": {"native_stage_statistics": true, "native_machine_preferences": true,
                "native_update_frames": true, "native_screen_lifecycle": true},
            "dropped_update_frames": 0, "dropped_events": 0, "dropped_stage_samples": 0,
            "completion": {"reason": "gameplay-exit", "stage_committed": true},
            "update_frame_phase": "gameplay-overlay-probe", "input": {"injected_inputs": false},
            "update_frames": [{"sequence": 1}, {"sequence": 2}],
            "events": [{"seq": 1, "kind": "call"}], "stage_samples": [{}]
        });
        super::validate_capture(&complete).expect("complete executable observations");
        assert_eq!(super::native_random_seed(&complete).expect("legacy seed"), 1);
        let mut seeded = complete.clone();
        seeded["capabilities"]["native_random_seed"] = serde_json::json!(true);
        assert!(super::validate_capture(&seeded).is_err());
        seeded["random_seed"] = serde_json::json!(73);
        seeded["random_reseeds"] = serde_json::json!([]);
        seeded["dropped_random_reseeds"] = serde_json::json!(0);
        super::validate_capture(&seeded).expect("declared native random input");
        assert_eq!(super::native_random_seed(&seeded).expect("recorded seed"), 73);
        for value in [serde_json::json!(0), serde_json::json!(2147483648_u64), serde_json::json!(-1), serde_json::json!(null)] {
            seeded["random_seed"] = value;
            assert!(super::validate_capture(&seeded).is_err());
        }
        seeded["random_seed"] = serde_json::json!(1);
        seeded["dropped_random_reseeds"] = serde_json::json!(1);
        assert!(super::validate_capture(&seeded).is_err());
        let mut initial = complete.clone();
        initial["capabilities"]["native_display_preference"] = serde_json::json!(true);
        initial["capabilities"]["native_initial_player_options"] = serde_json::json!(true);
        initial["input"]["preferences"] = serde_json::json!({"display_aspect_ratio": 16.0 / 9.0});
        initial["input"]["gameplay_preferences"] = serde_json::json!({"players": [
            {"player": "PlayerNumber_P1", "options_string": "NoHideLights, PlayerAutoPlay, Overhead"}
        ]});
        super::validate_capture(&initial).expect("recorded native startup inputs");
        for ratio in [serde_json::json!(0), serde_json::json!(4.0 / 3.0)] {
            initial["input"]["preferences"]["display_aspect_ratio"] = ratio;
            super::validate_capture(&initial).expect("native configured display ratio");
        }
        for (path, replacement) in [
            ("/input/preferences/display_aspect_ratio", serde_json::json!(null)),
            ("/input/preferences/display_aspect_ratio", serde_json::json!(-1)),
            ("/input/preferences/display_aspect_ratio", serde_json::json!(1e308)),
            ("/input/gameplay_preferences/players", serde_json::json!([])),
            ("/input/gameplay_preferences/players/0/options_string", serde_json::json!(null)),
        ] {
            let mut incomplete = initial.clone();
            *incomplete.pointer_mut(path).expect("recorded initial input") = replacement;
            assert!(super::validate_capture(&incomplete).is_err(), "accepted {path}");
        }
        for (path, replacement) in [
            (
                "/capabilities/native_update_frames",
                serde_json::json!(false),
            ),
            ("/dropped_update_frames", serde_json::json!(1)),
            ("/dropped_events", serde_json::json!(1)),
            ("/dropped_stage_samples", serde_json::json!(1)),
            ("/completion/reason", serde_json::json!("until-beat")),
            ("/completion/stage_committed", serde_json::json!(false)),
            ("/input/injected_inputs", serde_json::json!(true)),
            ("/update_frames", serde_json::json!([])),
            ("/update_frames/1/sequence", serde_json::json!(3)),
            ("/events/0/seq", serde_json::json!(2)),
            ("/events/0/kind", serde_json::json!("error")),
            ("/stage_samples", serde_json::json!([])),
        ] {
            let mut incomplete = complete.clone();
            *incomplete.pointer_mut(path).expect("control field") = replacement;
            assert!(
                super::validate_capture(&incomplete).is_err(),
                "accepted {path}"
            );
        }
        let mut empty_commands = complete.clone();
        empty_commands["events"] = serde_json::json!([]);
        assert!(super::validate_capture(&empty_commands).is_ok(),
            "a complete native run can execute an empty actor branch");
    }

    #[test]
    fn gameplay_music_end_uses_the_whole_song_before_the_resolution_window() {
        use super::{gameplay, paths, GameplayConfig, GameplaySession, GameplayViewport};
        use std::{fs, sync::Arc};
        paths::init();
        let dir = std::env::temp_dir().join(format!("deadsync-whole-song-end-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let simfile = dir.join("end.ssc");
        // Native Song.cpp includes fake rows on other playable charts in
        // GetLastSecond, then ScreenGameplay.cpp adds the resolution window.
        for (pack_offsets, sync, hint) in [
            (false, deadsync_chart::SyncPref::Itg, 0.0_f32),
            (true, deadsync_chart::SyncPref::Null, 0.0),
            (true, deadsync_chart::SyncPref::Itg, 0.0),
            (true, deadsync_chart::SyncPref::Itg, 4.004),
            (true, deadsync_chart::SyncPref::Itg, 8.0),
        ] {
        fs::write(&simfile, format!("#LASTSECONDHINT:{hint};\n#VERSION:0.83;\n#TITLE:Whole song end;\n#BPMS:0=60;\n#FAKES:3=8;\n\
#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DESCRIPTION:Selected;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n1000\n0000\n0000\n1000\n;\n\
#NOTEDATA:;\n#STEPSTYPE:dance-double;\n#DESCRIPTION:Later fake;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n10000000\n00000000\n00000000\n00000000\n,\n10000000\n00000000\n00000000\n00000000\n;\n")).unwrap();
        let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap());
        assert_eq!(song.precise_last_second(), 4.0_f32.max(hint));
        let chart = Arc::new(song.charts[0].clone());
        let payload = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], 0.0).unwrap().remove(0));
        let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
        let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
        let mut state = gameplay::init(song, [chart.clone(), chart], [payload.clone(), payload],
            GameplayViewport::default(), GameplaySession::default(), GameplayConfig { machine_pack_ini_offsets: pack_offsets, machine_default_sync_pref: sync, ..Default::default() },
            5, 1.0, scroll, profiles, None, None, None, Arc::from("EVENT"),
            None, None, None, None, None, [deadsync_gameplay::CourseLifeConfig::Bar; 2],
            false, None, None, [0; 2], Default::default(), Default::default());
        for rate in [1.0, 1.5, 0.75] {
            if rate != 1.0 {
                assert!(state.set_music_rate(rate));
            }
            // The native roll window is the largest window in this profile.
            // Song.cpp applies the group offset while converting note beats.
            // LASTSECONDHINT remains an absolute music second.
            let group_offset = if pack_offsets && sync == deadsync_chart::SyncPref::Itg { -0.009_f32 } else { 0.0 };
            let last = (4.0_f32 - group_offset).max(hint);
            let expected = (f64::from(last) * 1e9).round() as i64 +
                (f64::from(deadsync_rules::note::TIMING_WINDOW_SECONDS_ROLL * rate) * 1e9).round() as i64;
            assert_eq!(state.music_end_time_ns(), expected, "whole-song end includes group sync before the absolute hint");
            assert!(state.apply_global_offset_delta(0.125));
            assert_eq!(state.music_end_time_ns(), expected, "whole-song end includes group sync before the absolute hint");
            state.set_current_music_time_ns(expected - 1);
            assert!(state.finish_gameplay_if_ready(false).is_none());
        }
        let end = state.music_end_time_ns();
        state.set_current_music_time_ns(end);
        assert!(matches!(state.finish_gameplay_if_ready(false),
            Some(deadsync_gameplay::GameplayAction::Navigate(deadsync_gameplay::GameplayExit::Complete))));
        }
    }

    #[test]
    fn gameplay_music_fade_finishes_before_the_native_transition() {
        use super::{gameplay, paths, GameplayConfig, GameplaySession, GameplayViewport};
        use std::{fs, sync::Arc};
        paths::init();
        let dir = std::env::temp_dir().join(format!("deadsync-music-fade-end-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let simfile = dir.join("end.ssc");
        fs::write(&simfile, "#VERSION:0.83;#TITLE:Music fade end;#BPMS:0=60;\n\
#NOTEDATA:;#STEPSTYPE:dance-single;#DESCRIPTION:Selected;#DIFFICULTY:Hard;#METER:1;#NOTES:\n1000\n0000\n0000\n1000\n;\n\
#NOTEDATA:;#STEPSTYPE:dance-double;#DESCRIPTION:Later;#DIFFICULTY:Hard;#METER:1;#NOTES:\n10000000\n00000000\n00000000\n00000000\n,\n10000000\n00000000\n00000000\n00000000\n;\n").unwrap();
        for audio_length in [8.0_f32, 9.2, 10.0] {
            let mut song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap());
            Arc::make_mut(&mut song).music_length_seconds = audio_length;
            let chart = Arc::new(song.charts[0].clone());
            let payload = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], 0.0).unwrap().remove(0));
            let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
            let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
            let mut state = gameplay::init(song, [chart.clone(), chart], [payload.clone(), payload],
                GameplayViewport::default(), GameplaySession::default(), GameplayConfig::default(),
                5, 1.0, scroll, profiles, None, None, None, Arc::from("EVENT"), None, None, None,
                None, None, [deadsync_gameplay::CourseLifeConfig::Bar; 2], false, None, None,
                [0; 2], Default::default(), Default::default());
            for rate in [1.0_f32, 1.5, 0.75] {
                if rate != 1.0 { assert!(state.set_music_rate(rate)); }
                // ScreenGameplay::GetMusicEndTiming uses the declared five
                // seconds and Simply Love's 0.25-second music fade. Its actual
                // visual out tween still runs separately for 1.5 seconds.
                let note_end = 4.0_f32 + deadsync_rules::note::TIMING_WINDOW_SECONDS_ROLL * rate;
                let fade_start = if note_end + 5.0 < audio_length { note_end + 5.0 - 0.25 }
                    else { audio_length };
                let expected = note_end.max(fade_start.max(note_end) + 0.25 - 5.0);
                let actual = (state.music_end_time_ns() as f64 / 1e9) as f32;
                assert_eq!(actual, expected, "native music fade constraint retains the audio EOF boundary");
                assert!(state.apply_global_offset_delta(0.125));
                assert_eq!((state.music_end_time_ns() as f64 / 1e9) as f32, expected,
                    "player offset edits retain the cached song and music fade boundary");
            }
        }
    }

    #[test]
    fn gameplay_empty_declared_layers_retains_owned_theme_scene() {
        paths::init();
        let dir = std::env::temp_dir().join(format!("deadsync-empty-declared-layers-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lua")).unwrap();
        let simfile = dir.join("empty.sm");
        std::fs::write(&simfile, "#TITLE:Empty declared layers;#BPMS:0=60;#BGCHANGES:;#FGCHANGES:;#NOTES:dance-single:Control:Hard:1:0,0,0,0,0:\n1000\n0000\n0000\n0000\n;").unwrap();
        std::fs::write(dir.join("lua/default.lua"), "error('unreferenced song Lua must not execute')").unwrap();
        let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap());
        assert!(song.background_lua_changes.is_empty() && song.foreground_lua_changes.is_empty());
        let chart = Arc::new(song.charts[0].clone());
        let payload = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], 0.0).unwrap().remove(0));
        let mut context = deadsync_song_lua::SongLuaCompileContext::new(&dir, "empty declared layers");
        let mut inactive = deadsync_song_lua::playback::prepare_song_lua(
            &song, &context, deadsync_assets::song_lua::compile_song_lua,
            deadsync_assets::song_lua::compile_song_lua_layers);
        assert!(inactive.take_live_scene().unwrap().is_none(), "non-live empty layers need no VM");
        context.live = true;
        context.defer_initial_live_update = true;
        context.song_timing = song.song_timing.clone();
        let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
        let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
        let prepared = deadsync_song_lua::playback::prepare_song_lua(
            &song, &context, deadsync_assets::song_lua::compile_song_lua,
            deadsync_assets::song_lua::compile_song_lua_layers);
        let mut state = gameplay::init(song, [chart.clone(), chart], [payload.clone(), payload],
            GameplayViewport::default(), GameplaySession::default(), GameplayConfig::default(),
            5, 1.0, scroll, profiles, None, None, None, Arc::from("EVENT"), None, None, None,
            None, None, [deadsync_gameplay::CourseLifeConfig::Bar; 2], false, None, None,
            [0; 2], prepared, Default::default());
        assert!(state.song_lua.is_some(), "empty declared layers retain the owned gameplay scene");
        let scene = state.song_lua.as_ref().unwrap();
        assert!(scene.matches_entries(&[], 0), "no authored entry is invented");
        assert!(scene.context().foreground_names.is_empty() && scene.context().layer_start_beats.is_empty());
        assert_eq!(scene.states().len(), 1, "the screen has no authored actor overlay");
        let startup = state.take_song_lua_output();
        assert!(startup.errors.is_empty() && startup.source_broadcasts.is_empty());
        assert_eq!(startup.broadcasts.iter().filter(|(_, name, _)| name == "ScreenChanged").count(), 1);
        state.set_live_autoplay_enabled(true);
        let mut navigated = false;
        for index in 0..80 {
            let at = index * 125_000_000;
            let host = at as u64 + 1_000_000_000;
            let mut effects = Vec::new();
            gameplay::update(&mut state, if index == 0 { 0.0 } else { 0.125 }, GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot { music_nanos: at,
                    has_music_mapping: true, valid_at_host_nanos: host, ..Default::default() },
                ..Default::default()
            }, || host, &mut effects);
            let output = state.take_song_lua_output();
            assert!(output.errors.is_empty() && output.source_broadcasts.is_empty(), "{:?}", output.errors);
            if effects.iter().any(|effect| matches!(effect,
                deadsync_theme_simply_love::SimplyLoveEffect::NavigateNoFade(deadsync_theme_simply_love::screens::SimplyLoveScreen::Evaluation))) {
                navigated = true;
                break;
            }
        }
        assert!(navigated, "the owned scene reaches natural navigation");
        assert!(state.gameplay.players()[0].stage_failed, "owned theme Off fails judged autoplay");
    }

    fn owned_judgment_control(name: &str, notes: &str, script: &str, offset: f32) -> gameplay::State {
        owned_judgment_control_with_attacks(name, notes, script, offset, "")
    }

    fn owned_judgment_control_with_attacks(name: &str, notes: &str, script: &str, offset: f32, attacks: &str) -> gameplay::State {
        paths::init();
        let dir = std::env::temp_dir().join(format!("deadsync-judgment-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let simfile = dir.join("control.sm");
        std::fs::write(&simfile, format!("#TITLE:{name};#OFFSET:{offset};#BPMS:0=144;#ATTACKS:{attacks};#FGCHANGES:-24=default.lua=1=0=0=1;#NOTES:dance-single:Control:Hard:1:0,0,0,0,0:\n{notes};")).unwrap();
        std::fs::write(dir.join("default.lua"), script).unwrap();
        let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, -0.008).unwrap());
        let chart = Arc::new(song.charts[0].clone());
        let payload = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], -0.008).unwrap().remove(0));
        let mut context = deadsync_song_lua::SongLuaCompileContext::new(&dir, name);
        context.live = true;
        context.defer_initial_live_update = true;
        context.players[1].enabled = false;
        context.song_timing = song.song_timing.clone();
        let profiles = std::array::from_fn(|_| {
            let mut profile = deadsync_profile::Profile::default();
            if !attacks.is_empty() { profile.attack_mode = deadsync_profile::AttackMode::On; }
            profile
        });
        let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
        let prepared = if script.is_empty() {
            Default::default()
        } else {
            deadsync_song_lua::playback::prepare_song_lua(
                &song, &context, deadsync_assets::song_lua::compile_song_lua,
                deadsync_assets::song_lua::compile_song_lua_layers)
        };
        let mut state = gameplay::init(song, [chart.clone(), chart], [payload.clone(), payload],
            GameplayViewport::default(), GameplaySession::default(), GameplayConfig {
                global_offset_seconds: -0.008,
                machine_default_sync_pref: deadsync_chart::SyncPref::Itg,
                machine_pack_ini_offsets: true,
                ..Default::default()
            }, 5, 1.0, scroll, profiles, None, None, None, Arc::from("EVENT"), None, None, None,
            None, None, [deadsync_gameplay::CourseLifeConfig::Bar; 2], false, None, None,
            [0; 2], prepared, Default::default());
        state.set_live_autoplay_enabled(true);
        state
    }

    #[test]
    fn gameplay_hardware_input_drives_live_text_and_releases() {
        use deadsync_input::{VirtualAction, keymap::{InputState, InputBinding, Keymap}};
        use deadlib_platform::input::{RawKeyboardEvent, KeyCode};
        use std::time::Instant;
        let mut state = owned_judgment_control("interactive-hardware", "0000\n0000\n0000\n0000", r#"
local text
local presses, repeats, releases = 0,0,0
return Def.ActorFrame {
 OnCommand=function(self)
  SCREENMAN:GetTopScreen():AddInputCallback(function(e)
   assert(e.button == 'Right' and e.GameButton == 'MenuRight')
   assert(e.DeviceInput.device == 'InputDevice_Key' and e.DeviceInput.button == 'DeviceButton_right')
   if e.type == 'InputEventType_FirstPress' then presses=presses+1
   elseif e.type == 'InputEventType_Repeat' then repeats=repeats+1
   elseif e.type == 'InputEventType_Release' then releases=releases+1 end
   text:settext(presses..':'..repeats..':'..releases)
  end)
 end,
 Def.BitmapText {Font='Common Normal',InitCommand=function(self) text=self; self:settext('0:0:0') end},
}
"#, -0.008);
        update_judgment_control(&mut state, 0, 1_000_000);
        let mut keymap=Keymap::default();
        keymap.bind(VirtualAction::p1_right, &[InputBinding::Key(KeyCode::ArrowRight)]);
        let mut input=InputState::new(&keymap,0.0);
        let now=Instant::now();
        let mut edge=|state: &mut gameplay::State, pressed| {
            let events=input.map_key(input.key_event(RawKeyboardEvent {code:KeyCode::ArrowRight,pressed,repeat:false,
                timestamp:now,host_nanos:1_000_000}),||now).collect::<Vec<_>>();
            for event in events { gameplay::handle_input_with_device(state,&event,input.physical_input(&event),&mut Vec::new()); }
        };
        let find_text=|s: &gameplay::State| s.gameplay.mods.song_lua_visuals.overlays.iter()
            .chain(s.gameplay.mods.song_lua_visuals.foreground_visual_layers.iter().flat_map(|l|l.overlays.iter()))
            .find_map(|a|if let deadsync_assets::song_lua::SongLuaOverlayKind::BitmapText {text,..}=&a.kind {Some(text.to_string())} else {None}).unwrap();
        edge(&mut state,true);
        assert_eq!(find_text(&state),"0:0:0","physical callbacks wait for the post-update input phase");
        gameplay::update(&mut state,0.0,GameplayAudioSnapshot::default(),||1_000_001,&mut Vec::new());
        assert_eq!(find_text(&state),"1:0:0");
        gameplay::update(&mut state,0.4,GameplayAudioSnapshot::default(),||2_000_000,&mut Vec::new());
        assert_eq!(find_text(&state),"1:1:0");
        edge(&mut state,false);
        gameplay::update(&mut state,0.0,GameplayAudioSnapshot::default(),||2_000_001,&mut Vec::new());
        assert_eq!(find_text(&state),"1:1:1");
        gameplay::update(&mut state,0.5,GameplayAudioSnapshot::default(),||3_000_000,&mut Vec::new());
        assert_eq!(find_text(&state),"1:1:1","release stops repeats and does not call back twice for a menu alias");
        edge(&mut state,true);
        gameplay::update(&mut state,0.0,GameplayAudioSnapshot::default(),||3_000_001,&mut Vec::new());
        state.clear_song_lua_inputs();
        assert_eq!(find_text(&state),"2:1:2","focus loss releases held callback buttons");
        assert!(state.take_song_lua_output().errors.is_empty());
    }

    #[test]
    fn gameplay_live_song_fail_setting_controls_real_navigation() {
        use deadsync_gameplay::GameplayFailType;
        use deadsync_theme_simply_love::SimplyLoveEffect;
        for (name, setter, expected) in [
            ("continue", ":FailSetting('FailType_ImmediateContinue')", GameplayFailType::ImmediateContinue),
            ("off", ":FromString('FailOff')", GameplayFailType::Off),
        ] {
            let script = format!("GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song'){setter}\nreturn Def.Actor{{}}");
            let mut state = owned_judgment_control(name, "1000\n0000\n0000\n0001\n", &script, 0.0);
            state.gameplay.setup.config.default_fail_type = GameplayFailType::Immediate;
            state.gameplay.players_runtime.players[0].life = 0.0;
            for index in 0..3 {
                let host = 1_000_000_000 + index * 2_000_000;
                let mut effects = Vec::new();
                gameplay::update(&mut state, 0.002, GameplayAudioSnapshot {
                    stream_clock: GameplayStreamClockSnapshot { music_nanos: -250_000_000 + index as i64 * 2_000_000,
                        has_music_mapping: true, valid_at_host_nanos: host, ..Default::default() },
                    ..Default::default()
                }, || host, &mut effects);
                let output = state.take_song_lua_output();
                assert!(output.errors.is_empty(), "{:?}", output.errors);
                assert!(effects.iter().all(|effect| !matches!(effect,
                    SimplyLoveEffect::Navigate(_) | SimplyLoveEffect::NavigateNoFade(_))),
                    "{name}: a live Song fail policy must prevent the core's immediate failure exit");
                assert_eq!(state.gameplay.players()[0].fail_type, expected,
                    "{name}: attack refresh must retain the live option policy");
            }
        }
    }

    fn update_judgment_control(state: &mut gameplay::State, nanos: i64, host: u64) -> deadsync_song_lua::SongLuaLiveOutput {
        gameplay::update(state, 0.002, GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot { music_nanos: nanos,
                has_music_mapping: true, valid_at_host_nanos: host, ..Default::default() },
            ..Default::default()
        }, || host, &mut Vec::new());
        let output = state.take_song_lua_output();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        output
    }

    #[test]
    fn gameplay_out_transition_resolves_latest_explicit_next_screen() {
        use deadsync_theme_simply_love::{SimplyLoveEffect as Effect, SimplyLoveRuntimeRequest as Runtime};
        use deadsync_theme_simply_love::screens::SimplyLoveScreen;
        let notes = std::iter::repeat_n("1000\n0000\n0000\n0000\n", 32).collect::<Vec<_>>().join(",\n");
        for target in ["ScreenGameplay", "ScreenSelectMusic"] {
            let script = format!(r#"
local screen = SCREENMAN:GetTopScreen()
local changed = false
return Def.ActorFrame {{
    OnCommand = function(self)
        self:sleep(9999)
        screen:SetNextScreenName('ScreenEvaluationStage')
        screen:StartTransitioningScreen('SM_DoNextScreen')
        self:SetUpdateFunction(function()
            if not changed and GAMESTATE:GetCurMusicSeconds() >= 0.25 then
                changed = true
                screen:SetNextScreenName('{target}')
            end
        end)
    end,
    OffCommand = function() MESSAGEMAN:Broadcast('TargetOff') end,
    EndCommand = function()
        assert(SCREENMAN:GetTopScreen() == nil, 'screen is popped before End')
        MESSAGEMAN:Broadcast('TargetEnd')
    end,
}}
"#);
            let mut state = owned_judgment_control(target, &notes, &script, 0.0);
            let mut lifecycle = Vec::new();
            let mut completion = None;
            for index in 0..32 {
                let nanos = index * 125_000_000;
                let mut effects = Vec::new();
                gameplay::update(&mut state, if index == 0 { 0.0 } else { 0.125 }, GameplayAudioSnapshot {
                    stream_clock: GameplayStreamClockSnapshot { music_nanos: nanos,
                        has_music_mapping: true, valid_at_host_nanos: nanos as u64 + 1_000_000_000,
                        ..Default::default() }, ..Default::default()
                }, || nanos as u64 + 1_000_000_000, &mut effects);
                let output = state.take_song_lua_output();
                assert!(output.errors.is_empty(), "{:?}", output.errors);
                lifecycle.extend(output.source_broadcasts.iter()
                    .filter(|event| matches!(event.name.as_str(), "TargetOff" | "TargetEnd"))
                    .map(|event| event.name.clone()));
                if !effects.is_empty() {
                    assert!(nanos >= 1_500_000_000, "screen load waits for the native out tween");
                    completion = Some(effects);
                    break;
                }
            }
            assert_eq!(lifecycle, ["TargetOff", "TargetEnd"], "Off and popped End run once before loading");
            let effects = completion.expect("out transition completes");
            let matches_target = match effects.as_slice() {
                [Effect::Runtime(Runtime::ReloadSongLua)] => target == "ScreenGameplay",
                [Effect::NavigateNoFade(SimplyLoveScreen::SelectMusic)] => target == "ScreenSelectMusic",
                _ => false,
            };
            assert!(matches_target, "explicit target must select completed transition destination: {effects:?}");
            let mut effects = Vec::new();
            gameplay::update(&mut state, 1.0, GameplayAudioSnapshot::default(), || 100_000_000_000, &mut effects);
            assert!(effects.is_empty() && state.take_song_lua_output().source_broadcasts.is_empty(),
                "completed transition is consumed once");
        }
    }

    #[test]
    fn gameplay_chart_attack_rebuilds_song_options() {
        let notes = std::iter::repeat_n("1000\n0000\n0000\n0000\n", 24).collect::<Vec<_>>().join(",\n");
        let mut state = owned_judgment_control_with_attacks("attack-option-levels", &notes, r#"
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(9999)
        local p1 = GAMESTATE:GetPlayerState(PLAYER_1)
        local p2 = GAMESTATE:GetPlayerState(PLAYER_2)
        p1:SetPlayerOptions('ModsLevel_Stage', '25% Dark')
        p1:SetPlayerOptions('ModsLevel_Song', '75% Drunk, 25% Tornado, Tipsy')
        p2:SetPlayerOptions('ModsLevel_Song', '50% Drunk')
        local song = p1:GetPlayerOptions('ModsLevel_Song')
        self:SetUpdateFunction(function()
            local seconds = GAMESTATE:GetCurMusicSeconds()
            assert(p1:GetPlayerOptions('ModsLevel_Song') == song, 'option object is retained')
            assert(p2:GetPlayerOptions('ModsLevel_Song'):Drunk() == 0.5, 'disabled P2 has no attack')
            if seconds > 2.81 then
                assert(song:Drunk() == 0 and song:Tornado() == 0 and song:Tipsy() == 0,
                    'chart attack boundary rebuilds Song from Stage')
                assert(song:Dark() == 0.25, 'Stage options survive the rebuild')
                MESSAGEMAN:Broadcast('AttackOptionsRebuilt')
            else
                assert(song:Drunk() == 0.75, 'Song writes survive before attack activation')
            end
        end)
    end,
}
"#, 0.0, "TIME=2.802432:LEN=0.405405:MODS=Overhead, Onlyonecouples");
        assert_eq!(state.gameplay.mods.attacks.option_windows[0].len(), 1);
        assert!(state.gameplay.mods.attacks.option_windows[1].is_empty());
        update_judgment_control(&mut state, -250_000_000, 1_000_000_000);
        update_judgment_control(&mut state, 1_000_000_000, 2_000_000_000);
        update_judgment_control(&mut state, 2_000_000_000, 3_000_000_000);
        assert!(!state.gameplay.mods.attacks.cleared_for_outro,
            "control ended before attack: last={}", state.gameplay.source.song.precise_last_second());
        for seconds in [3_i64, 4] {
            let output = update_judgment_control(&mut state, seconds * 1_000_000_000, (seconds as u64 + 1) * 1_000_000_000);
            assert!(output.source_broadcasts.iter().any(|event| event.name == "AttackOptionsRebuilt"),
                "music={} output={output:?}", state.gameplay.clock.song_position.current_music_time_ns);
        }
    }

    #[test]
    fn gameplay_immediate_failure_draws_until_native_blank_transition_finishes() {
        use deadsync_theme_simply_love::SimplyLoveEffect as Effect;
        use deadsync_theme_simply_love::screens::SimplyLoveScreen;
        let mut state = owned_judgment_control("immediate-failure-draw", "0000\n1000\n0000\n1000\n", r#"
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
return Def.ActorFrame {
    OnCommand=function(self)
        self:sleep(9999)
        self:SetDrawFunction(function() MESSAGEMAN:Broadcast('FailureDraw') end)
        self:addcommand('JudgmentMessage', function(_, params)
            if params.TapNoteScore == 'TapNoteScore_W1' then
                player:SetLife(0)
                STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1):FailPlayer()
            end
        end)
    end,
    OffCommand=function() MESSAGEMAN:Broadcast('FailureOff') end,
    EndCommand=function() MESSAGEMAN:Broadcast('FailureEnd') end,
}
"#, 0.0);
        state.gameplay.setup.config.default_fail_type = deadsync_gameplay::GameplayFailType::Immediate;
        update_judgment_control(&mut state, -250_000_000, 1_000_000_000);
        let time = state.gameplay.chart_runtime.note_time_cache_ns[0] + 1_000_000;
        for frame in 0..4 {
            let host = 2_000_000_000 + frame * 2_000_000;
            let mut effects = Vec::new();
            gameplay::update(&mut state, 0.002, GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot { music_nanos: time,
                    has_music_mapping: true, valid_at_host_nanos: host, ..Default::default() },
                ..Default::default()
            }, || host, &mut effects);
            let output = state.take_song_lua_output();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let names: Vec<_> = output.source_broadcasts.iter().map(|event| event.name.as_str()).collect();
            assert_eq!(names, match frame {
                0 | 1 => vec!["FailureDraw"],
                2 => vec!["FailureOff", "FailureDraw"],
                _ => vec!["FailureEnd"],
            }, "native queued failure phase {frame}");
            if frame < 3 {
                assert!(effects.is_empty(), "failure stays on screen through the last draw");
            } else {
                assert!(matches!(effects.as_slice(), [Effect::NavigateNoFade(SimplyLoveScreen::Evaluation)]));
            }
        }
    }

    #[test]
    fn native_tap_score_ranks_drive_scripted_judgment_life_changes() {
        let mut state = owned_judgment_control("tap-score-ranks", "0000\n1000\n0000\n1000\n", r#"
local ranks = TapNoteScore:Reverse()
assert(ranks.TapNoteScore_None == 0 and ranks.TapNoteScore_W4 == 6)
assert(ranks.TapNoteScore_W1 == 9 and ranks.TapNoteScore_CheckpointHit == 10)
local replacements = {[ranks.TapNoteScore_W1] = ranks.TapNoteScore_W4}
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
local meter = SCREENMAN:GetTopScreen():GetLifeMeter(PLAYER_1)
return Def.ActorFrame {
    OnCommand=function(self)
        self:sleep(9999)
        self:addcommand('JudgmentMessage', function(_, params)
            local score = ranks[params.TapNoteScore]
            if score and score > 6 then
                local replacement = TapNoteScore[replacements[score] + 1]
                assert(replacement == 'TapNoteScore_W4')
                player:SetLife(meter:GetLife()
                    - THEME:GetMetric('LifeMeterBar', 'LifePercentChangeW1')
                    + THEME:GetMetric('LifeMeterBar', 'LifePercentChangeW4') * 0.8)
                MESSAGEMAN:Broadcast('RankedJudgmentApplied')
            end
        end)
    end,
}
"#, 0.0);
        update_judgment_control(&mut state, -250_000_000, 1_000_000_000);
        let times = state.gameplay.chart_runtime.note_time_cache_ns.clone();
        assert_eq!(times.len(), 2);
        for (index, time) in times.into_iter().enumerate() {
            let output = update_judgment_control(&mut state, time + 1_000_000,
                2_000_000_000 + index as u64 * 1_000_000_000);
            assert_eq!(output.source_broadcasts.iter()
                .filter(|event| event.name == "RankedJudgmentApplied").count(), 1);
            // Release autoplay suppresses the automatic life gain, while the
            // chart's rank-based JudgmentMessage still subtracts its W1 metric.
            let expected_life = 0.5 - 0.008 * (index + 1) as f32;
            assert!((state.gameplay.players()[0].life - expected_life).abs() < 1e-6,
                "native score ranks must drive each scripted life change");
        }
    }

    #[test]
    fn gameplay_autoplay_judges_at_native_float_row_boundary() {
        let notes = (0..90).map(|measure| (0..16).map(|row| {
            let beat = measure as f32 * 4.0 + row as f32 / 4.0;
            if [38.5, 92.5, 356.0].contains(&beat) { "1000\n" } else { "0000\n" }
        }).collect::<String>()).collect::<Vec<_>>().join(",\n");
        let mut state = owned_judgment_control("float-row", &notes, r#"
return Def.ActorFrame {
    OnCommand = function(self) self:sleep(9999) end,
    JudgmentMessageCommand = function(_, params)
        assert(params.Player == PLAYER_1 and params.TapNoteScore == 'TapNoteScore_W1')
        MESSAGEMAN:Broadcast('FloatRowJudgment', {beat=GAMESTATE:GetSongBeat()})
    end,
}
"#, -0.409);
        update_judgment_control(&mut state, -250_000_000, 1_000_000_000);
        for (index, (beat, seconds)) in [(38.5_f32, 16.46766471862793_f64),
            (92.5, 38.9676628112793), (356.0, 148.7593231201172)].into_iter().enumerate() {
            let nanos = (seconds * 1e9).round() as i64;
            let before = update_judgment_control(&mut state, nanos - 2_000_000, 2_000_000_000 + index as u64 * 2_000_000);
            assert!(!before.source_broadcasts.iter().any(|event| event.name == "FloatRowJudgment"));
            assert!(state.gameplay.chart_runtime.note_time_cache_ns[index] > nanos,
                "the control must exercise a future nanosecond deadline");
            assert_eq!(state.gameplay.timing_runtime.timing_players[0].get_song_position(seconds as f32).beat, beat);
            let output = update_judgment_control(&mut state, nanos, 2_001_000_000 + index as u64 * 2_000_000);
            assert_eq!(output.source_broadcasts.iter().filter(|event| event.name == "FloatRowJudgment").count(), 1,
                "autoplay dispatches at native float row boundary: beat={beat}");
            assert!(state.gameplay.chart_runtime.notes[index].result.is_some());
        }
    }

    #[test]
    fn gameplay_avoided_mines_dispatch_each_owned_judgment_next_update() {
        let mut state = owned_judgment_control("mine-message", "0000\nM0MM\n0000\n0000\n,\n1000\n0000\n0000\n0000\n", r#"
local count = 0
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(9999)
        self:SetUpdateFunction(function()
            MESSAGEMAN:Broadcast('MineObserver', {count=count})
        end)
    end,
    JudgmentMessageCommand = function(_, params)
        if params.TapNoteScore ~= 'TapNoteScore_AvoidMine' then return end
        assert(params.Player == PLAYER_1)
        assert(params.NumTracks == nil and params.MultiPlayer == nil and params.Notes == nil)
        assert(params.Holds == nil and params.HoldNoteScore == nil and params.TapNoteOffset == nil)
        count = count + 1
        player:SetLife(0.125)
        MESSAGEMAN:Broadcast('AvoidMineJudgment', {column=params.FirstTrack, count=count})
    end,
}
"#, 0.0);
        update_judgment_control(&mut state, -250_000_000, 1_000_000_000);
        let mine_time = state.gameplay.chart_runtime.note_time_cache_ns[0];
        let avoided = mine_time + state.gameplay.timing_runtime.step_resolution_distance_ns + 20_000_000;
        let marking = update_judgment_control(&mut state, avoided, 2_000_000_000);
        assert!(state.gameplay.chart_runtime.notes[..3].iter().all(|note| note.mine_result == Some(deadsync_rules::note::MineResult::Avoided)));
        assert!(!marking.source_broadcasts.iter().any(|event| event.name == "AvoidMineJudgment"),
            "native UpdateJudgedRows precedes the avoidance marking pass");
        let output = update_judgment_control(&mut state, avoided + 2_000_000, 2_002_000_000);
        let columns: Vec<_> = output.source_broadcasts.iter().filter(|event| event.name == "AvoidMineJudgment")
            .map(|event| event.params.as_ref().unwrap().as_ref().unwrap()["column"].as_u64().unwrap()).collect();
        assert_eq!(columns, [0, 2, 3], "avoided mine Judgment dispatches every owned mine on the next update");
        assert_eq!(output.source_broadcasts.iter().find(|event| event.name == "MineObserver").unwrap()
            .params.as_ref().unwrap().as_ref().unwrap()["count"], json!(3), "mine messages precede foreground update");
        assert_eq!(state.gameplay.players()[0].life, 0.125, "synchronous Lua player writes reach gameplay");
        let later = update_judgment_control(&mut state, avoided + 4_000_000, 2_004_000_000);
        assert!(!later.source_broadcasts.iter().any(|event| event.name == "AvoidMineJudgment"), "each mine dispatches once");
    }

    #[test]
    fn gameplay_last_mine_judgment_survives_natural_outro() {
        let notes = "1000\n0000\n0000\nM000\n";
        let mut state = owned_judgment_control("last-mine-outro", notes, r#"
local count=0
return Def.Actor {
    OnCommand=function(self) self:sleep(9999) end,
    JudgmentMessageCommand=function(self, params)
        if params.TapNoteScore=='TapNoteScore_AvoidMine' then
            count=count+1
            MESSAGEMAN:Broadcast('FinalMineJudgment', {count=count})
        end
    end,
}
"#, 0.0);
        update_judgment_control(&mut state, -250_000_000, 1_000_000_000);
        let end = state.gameplay.music_end_time_ns();
        let mut judgments = Vec::new();
        let mut effects = Vec::new();
        let mut navigated = false;
        for step in -25_i64..=1700 {
            let now = end + step * 1_000_000;
            effects.clear();
            gameplay::update(&mut state, 0.001, GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot { music_nanos:now,
                    has_music_mapping:true, valid_at_host_nanos:(now+5_000_000_000) as u64,
                    ..Default::default() }, ..Default::default()
            }, || (now+5_000_000_000) as u64, &mut effects);
            let output=state.take_song_lua_output();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            judgments.extend(output.source_broadcasts.into_iter().filter(|event| event.name=="FinalMineJudgment"));
            if effects.iter().any(|effect| matches!(effect,
                deadsync_theme_simply_love::SimplyLoveEffect::NavigateNoFade(_))) {
                navigated=true;
                break;
            }
        }
        assert_eq!(judgments.len(), 1,
            "natural outro must keep the final mine's owned Judgment delivery");
        assert!(navigated, "the owned out transition must finish");
        assert_eq!(state.gameplay.players()[0].mines_avoided, 1);

        let mut ordinary = owned_judgment_control("last-mine-no-scene", notes, "", 0.0);
        assert!(ordinary.song_lua.is_none());
        update_judgment_control(&mut ordinary, -250_000_000, 1_000_000_000);
        let end=ordinary.gameplay.music_end_time_ns();
        let mut handed_off = false;
        for step in -25_i64..=25 {
            let now=end+step*1_000_000;
            effects.clear();
            gameplay::update(&mut ordinary, 0.001, GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot { music_nanos:now,
                    has_music_mapping:true, valid_at_host_nanos:(now+5_000_000_000) as u64,
                    ..Default::default() }, ..Default::default()
            }, || (now+5_000_000_000) as u64, &mut effects);
            if effects.iter().any(|effect| matches!(effect,
                deadsync_theme_simply_love::SimplyLoveEffect::Navigate(_))) {
                handed_off=true;
                break;
            }
        }
        assert!(handed_off, "ordinary gameplay must request its evaluation handoff");
        assert_eq!(ordinary.gameplay.players()[0].mines_avoided, 1,
            "ordinary gameplay must finalize counters before immediate evaluation handoff");
    }

    #[test]
    fn gameplay_natural_navigation_ends_owned_song_actors_once() {
        check_natural_navigation_ends_owned_song_actors_once(false);
    }

    #[test]
    fn gameplay_autoplay_overlay_off_keeps_successful_exit_clock() {
        check_natural_navigation_ends_owned_song_actors_once(true);
    }

    fn check_natural_navigation_ends_owned_song_actors_once(autoplay: bool) {
        use super::{gameplay, paths, GameplayAudioSnapshot, GameplayConfig,
            GameplaySession, GameplayStreamClockSnapshot, GameplayViewport};
        use deadsync_song_lua::SongLuaCompileContext;
        use std::{fs, sync::Arc};
        paths::init();
        let dir = std::env::temp_dir().join(format!("deadsync-natural-song-end-{}-{autoplay}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let simfile = dir.join("end.sm");
        fs::write(&simfile, "#TITLE:Natural owned End;#BPMS:0=60;#FGCHANGES:0=default.lua=1=0=0=1;#NOTES:dance-single:Control:Hard:1:0,0,0,0,0:\n1000\n0000\n0000\n0000\n;").unwrap();
        fs::write(dir.join("default.lua"), r#"
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local ends = 0
local off = false
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(9999)
        self:SetUpdateFunction(function()
            assert(off or not stats:GetFailed(), 'autoplay remains unfailed before owned Off')
        end)
        self:SetDrawFunction(function() MESSAGEMAN:Broadcast('AliveDraw') end)
    end,
    OffCommand = function()
        off = true
        assert(SCREENMAN:GetTopScreen() ~= nil)
        MESSAGEMAN:Broadcast('OwnedOff')
    end,
    Def.Sprite {
        OnCommand = function(self) self:sleep(0.125):queuecommand('End') end,
        EndCommand = function(self)
            ends = ends + 1
            local popped = SCREENMAN:GetTopScreen() == nil
            if popped then player:SetLife(0.375) end
            self:animate(false):linear(0.25):y(100)
            if popped then self:queuecommand('AfterEnd') end
            MESSAGEMAN:Broadcast('OwnedEnd', {count=ends, popped=popped})
        end,
        AfterEndCommand = function() MESSAGEMAN:Broadcast('AfterEnd') end,
    },
}
"#).unwrap();
        let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap());
        let chart = Arc::new(song.charts[0].clone());
        let payload = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], 0.0).unwrap().remove(0));
        let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
        let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
        let mut context = SongLuaCompileContext::new(&dir, "natural owned End");
        context.live = true;
        context.defer_initial_live_update = true;
        context.song_timing = song.song_timing.clone();
        let prepared = deadsync_song_lua::playback::prepare_song_lua(&song, &context,
            deadsync_assets::song_lua::compile_song_lua, deadsync_assets::song_lua::compile_song_lua_layers);
        let mut state = gameplay::init(song, [chart.clone(), chart], [payload.clone(), payload],
            GameplayViewport::default(), GameplaySession::default(), GameplayConfig::default(),
            5, 1.0, scroll, profiles, None, None, None, Arc::from("EVENT"), None, None, None,
            None, None, [deadsync_gameplay::CourseLifeConfig::Bar; 2], false, None, None,
            [0; 2], prepared, Default::default());
        state.set_live_autoplay_enabled(autoplay);
        let mut effects = Vec::new();
        let mut ends = Vec::new();
        let mut lifecycle = Vec::new();
        let mut after_end = 0;
        let mut navigated = false;
        for index in 0..512 {
            effects.clear();
            let at = index * 125_000_000;
            gameplay::update(&mut state, if index == 0 { 0.0 } else { 0.125 }, GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot { music_nanos: at,
                    has_music_mapping: true, valid_at_host_nanos: at as u64 + 1_000_000_000,
                    ..Default::default() }, ..Default::default()
            }, || at as u64 + 1_000_000_000, &mut effects);
            let output = state.take_song_lua_output();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            for event in &output.source_broadcasts {
                if event.name == "OwnedEnd" {
                    let params = event.params.as_ref().unwrap().as_ref().unwrap();
                    lifecycle.push(format!("End{}", params["count"]));
                    ends.push(params.clone());
                }
                if event.name == "OwnedOff" { lifecycle.push("Off".into()); }
                after_end += usize::from(event.name == "AfterEnd");
            }
            if effects.iter().any(|effect| matches!(effect,
                deadsync_theme_simply_love::SimplyLoveEffect::NavigateNoFade(deadsync_theme_simply_love::screens::SimplyLoveScreen::Evaluation))) {
                assert!(!output.source_broadcasts.iter().any(|event| event.name == "AliveDraw"),
                    "a popped screen is not drawn");
                navigated = true;
                break;
            }
        }
        assert_eq!(ends, vec![serde_json::json!({"count":1,"popped":false}), serde_json::json!({"count":2,"popped":true})],
            "natural navigation dispatches owned End a second time after the authored queue");
        assert!(navigated, "the real out transition must finish before popping the scene");
        assert_eq!(lifecycle, ["End1", "Off", "End2"], "Off precedes screen End");
        assert_eq!(state.gameplay.players()[0].life, 0.375, "End player writes reach the owned gameplay state");
        assert_eq!(after_end, 0, "screen End does not advance queues it creates");
        effects.clear();
        gameplay::update(&mut state, 1.0, GameplayAudioSnapshot::default(), || 100_000_000_000, &mut effects);
        let output = state.take_song_lua_output();
        assert!(output.source_broadcasts.is_empty() && effects.is_empty(), "screen End and draw run only once");
    }

    #[test]
    fn gameplay_note_field_tap_runs_owned_lua_before_actor_update() {
        use super::{gameplay, paths, GameplayAudioSnapshot, GameplayConfig,
            GameplaySession, GameplayStreamClockSnapshot, GameplayViewport};
        use deadsync_song_lua::SongLuaCompileContext;
        use std::{fs, sync::Arc};
        super::start_call_log(10_000).expect("observational logger");
        paths::init();
        let dir = std::env::temp_dir().join(format!("deadsync-engine-tap-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("script directory");
        let simfile = dir.join("tap.sm");
        fs::write(&simfile, "#TITLE:Engine tap;#BPMS:0=120;#FGCHANGES:-24=default.lua=1=0=0=1;#NOTES:dance-single:Control:Hard:1:0,0,0,0,0:\n0000\n2000\n3000\n0000\n;").unwrap();
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
local field = SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
local meter = SCREENMAN:GetTopScreen():GetLifeMeter(PLAYER_1)
local taps = 0
local judgments = 0
local steps = 0
return Def.ActorFrame {
    OnCommand = function(self)
        MESSAGEMAN:Broadcast('EngineStartup', {music=GAMESTATE:GetCurMusicSeconds()})
        self:sleep(60)
        field:SetDidTapNoteCallback(function(column, score, bright)
            assert(column == 1 and score == 'TapNoteScore_W1' and bright == false)
            local before = GAMESTATE:GetSongBeat()
            self:playcommand('TapNested')
            assert(GAMESTATE:GetSongBeat() == before)
            taps = taps + 1
            player:SetLife(0.25)
            assert(meter:GetLife() == 0.25 and stats:GetCurrentLife() == 0.25)
            self:stoptweening():sleep(0.25)
            MESSAGEMAN:Broadcast('EngineTap', {beat=before,
                combo=stats:GetCurrentCombo(), possible=stats:GetCurrentPossibleDancePoints(), left=self:GetTweenTimeLeft()})
            return 2, 'TapNoteScore_W2', true
        end)
        self:SetUpdateFunction(function(self, delta)
            MESSAGEMAN:Broadcast('EngineObserve', {left=self:GetTweenTimeLeft(), taps=taps, delta=delta})
        end)
    end,
    TapNestedCommand = function()
        MESSAGEMAN:Broadcast('EngineTapNested', {beat=GAMESTATE:GetSongBeat()})
    end,
    StepMessageCommand = function(_, params)
        assert(params.PlayerNumber == PLAYER_1 and params.Column == 0)
        assert(params.MultiPlayer == nil)
        assert(judgments == 0 and meter:GetLife() == 0.25)
        steps = steps + 1
        MESSAGEMAN:Broadcast('EngineStep', {steps=steps})
    end,
    StepP1MessageCommand = function(_, params)
        assert(type(params) == 'table' and next(params) == nil)
        assert(steps == 1)
        MESSAGEMAN:Broadcast('EngineStepP1')
    end,
    JudgmentMessageCommand = function(_, params)
        assert(params.Player == PLAYER_1 and params.FirstTrack == 0)
        assert(IsAutoplay(params.Player))
        if params.HoldNoteScore then
            assert(params.HoldNoteScore == 'HoldNoteScore_Held' and params.NumTracks == 4)
            assert(params.TapNoteScore == 'TapNoteScore_W1' and params.Notes == nil)
            assert(params.TapNote:GetTapNoteType() == 'TapNoteType_HoldHead')
            assert(meter:GetLife() == 0.125 and stats:GetCurrentPossibleDancePoints() == 0)
            player:SetLife(0.0625)
            MESSAGEMAN:Broadcast('EngineHold', {score=params.HoldNoteScore})
            return
        end
        assert(params.TapNoteScore == 'TapNoteScore_W1' and params.HoldNoteScore == nil)
        assert(params.Notes[1]:GetTapNoteType() == 'TapNoteType_HoldHead')
        assert(params.Notes[1]:GetHoldDuration() == 1)
        assert(params.Notes[1]:GetTapNoteResult():GetTapNoteScore() == 'TapNoteScore_W1')
        assert(type(params.Holds) == 'table' and next(params.Holds) == nil)
        assert(meter:GetLife() == 0.25 and stats:GetCurrentPossibleDancePoints() == 0)
        player:SetLife(0.125)
        judgments = judgments + 1
        MESSAGEMAN:Broadcast('EngineJudgment', {judgments=judgments})
    end,
    OffCommand = function(self)
        assert(not IsAutoplay(PLAYER_1))
        assert(stats:GetFailed())
        MESSAGEMAN:Broadcast('EngineOff', {music=GAMESTATE:GetCurMusicSeconds()})
    end,
}
"#).unwrap();
        let config = GameplayConfig::default();
        let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap());
        let chart = Arc::new(song.charts[0].clone());
        let payload = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], 0.0).unwrap().remove(0));
        let mut context = SongLuaCompileContext::new(&dir, "engine tap");
        context.live = true;
        context.defer_initial_live_update = true;
        context.song_timing = song.song_timing.clone();
        // Use production preparation and screen attachment, including their
        // concrete sprite and profile types; expectations never enter gameplay.
        let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
        let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
        let prepared = deadsync_song_lua::playback::prepare_song_lua(
            &song, &context, deadsync_assets::song_lua::compile_song_lua,
            deadsync_assets::song_lua::compile_song_lua_layers);
        let mut state = gameplay::init(song, [chart.clone(), chart], [payload.clone(), payload],
            GameplayViewport::default(), GameplaySession::default(), config, 5, 1.0, scroll, profiles,
            None, None, None, Arc::from("EVENT"), None, None, None, None, None,
            [deadsync_gameplay::CourseLifeConfig::Bar; 2], false, None, None, [0; 2], prepared, Default::default());
        state.set_live_autoplay_enabled(true);
        state.set_current_music_time_ns(0);
        let mut effects = Vec::new();
        let tap_time = state.gameplay.chart_runtime.note_time_cache_ns[0];
        gameplay::update(&mut state, 0.0, GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot { music_nanos: -250_000_000,
                has_music_mapping: true, valid_at_host_nanos: 800_000_000, ..Default::default() },
            ..Default::default()
        }, || 800_000_000, &mut effects);
        let first = state.take_song_lua_output();
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        let initial = first.source_broadcasts.iter().find(|event| event.name == "EngineStartup")
            .expect("zero-delta input activates the foreground");
        assert_eq!(initial.music_seconds, -0.25);
        assert_eq!(initial.params.as_ref().unwrap().as_ref().unwrap()["music"].as_f64(), Some(-0.25));
        // Foreground uses the music-position delta. Keep successive clock
        // inputs 100ms apart, as well as their wall-frame delta.
        gameplay::update(&mut state, 0.1, GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot { music_nanos: tap_time - 100_000_000,
                has_music_mapping: true, valid_at_host_nanos: 900_000_000, ..Default::default() },
            ..Default::default()
        }, || 900_000_000, &mut effects);
        let startup = state.take_song_lua_output();
        assert!(startup.errors.is_empty(), "{:?}", startup.errors);
        let expected_beat = context.song_timing.as_ref().unwrap()
            .get_song_position(tap_time as f32 / 1_000_000_000.0).beat;
        gameplay::update(&mut state, 0.1, GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot { music_nanos: tap_time,
                has_music_mapping: true, valid_at_host_nanos: 1_000_000_000, ..Default::default() },
            ..Default::default()
        }, || 1_000_000_000, &mut effects);
        let output = state.take_song_lua_output();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(state.gameplay.players_runtime.players[0].life, 0.125,
            "native judgment dispatch applies Lua writes before row scoring and foreground update");
        assert_eq!(output.source_broadcasts.iter().filter(|event| event.name == "EngineJudgment").count(), 1);
        let step_order: Vec<_> = output.source_broadcasts.iter().map(|event| event.name.as_str())
            .filter(|name| matches!(*name, "EngineJudgment" | "EngineStep" | "EngineStepP1" | "EngineObserve")).collect();
        assert_eq!(step_order, ["EngineStep", "EngineStepP1", "EngineJudgment", "EngineObserve"],
            "Player::Step broadcasts before row judgment and foreground update");
        assert_eq!(output.source_broadcasts.iter().filter(|event| event.name == "EngineTap").count(), 1,
            "clock={} notes={:?} scores={:?} scene={} hook={}",
            state.gameplay.clock.song_position.current_music_time_ns,
            state.gameplay.chart_runtime.note_time_cache_ns,
            state.gameplay.chart_runtime.notes.iter().map(|note| note.result).collect::<Vec<_>>(),
            state.song_lua.is_some(), state.gameplay.note_field_tap_callback.is_some());
        // Player.cpp updates its own actor children before judging. Custom
        // explosions in SongForeground update after that Player callback.
        let observation = output.source_broadcasts.iter().find(|event| event.name == "EngineObserve")
            .expect("foreground observer").params.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(observation["taps"], serde_json::json!(1), "callback precedes the foreground update");
        assert!((observation["left"].as_f64().unwrap() - 0.15).abs() < 1e-6,
            "new foreground tween consumes this frame's delta: observation={observation}, music_ns={}, tap={:?}",
            state.gameplay.clock.song_position.current_music_time_ns,
            output.source_broadcasts.iter().find(|event| event.name == "EngineTap"));
        let events = super::CALL_EVENTS.with_borrow(|capture| capture.events.clone());
        let queries: Vec<_> = events.iter().filter(|event| event["operation"] == "GameState.GetSongBeat").collect();
        assert_eq!(queries.len(), 3);
        for (query, phase) in queries.iter().zip(["construction", "callback", "construction"]) {
            assert_eq!(query["phase"], phase,
                "direct NoteField queries surround the nested owned command without borrowing its scope");
            assert_eq!(query["results"][0], serde_json::json!(expected_beat));
        }
        gameplay::update(&mut state, 0.1, GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot { music_nanos: tap_time + 100_000_000,
                has_music_mapping: true, valid_at_host_nanos: 1_100_000_000, ..Default::default() },
            ..Default::default()
        }, || 1_100_000_000, &mut effects);
        let next = state.take_song_lua_output();
        assert!(next.errors.is_empty(), "{:?}", next.errors);
        assert!(!next.source_broadcasts.iter().any(|event| event.name == "EngineJudgment"));
        assert!(!next.source_broadcasts.iter().any(|event| event.name.starts_with("EngineStep")));
        assert_eq!(state.gameplay.players_runtime.players[0].life, 0.125);
        let hold_time = state.gameplay.chart_runtime.hold_end_time_cache_ns[0];
        let delta = (hold_time - tap_time - 100_000_000) as f32 / 1_000_000_000.0;
        gameplay::update(&mut state, delta, GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot { music_nanos: hold_time,
                has_music_mapping: true, valid_at_host_nanos: 1_500_000_000, ..Default::default() },
            ..Default::default()
        }, || 1_500_000_000, &mut effects);
        let held = state.take_song_lua_output();
        assert!(held.errors.is_empty(), "{:?}", held.errors);
        assert_eq!(held.source_broadcasts.iter().filter(|event| event.name == "EngineHold").count(), 1);
        assert!(!held.source_broadcasts.iter().any(|event| event.name.starts_with("EngineStep")),
            "finishing a hold does not synthesize another press");
        assert_eq!(state.gameplay.players_runtime.players[0].life, 0.0625,
            "completed native hold judgments also reach the retained VM during autoplay");
        state.set_live_autoplay_enabled(false);
        let end = state.gameplay.music_end_time_ns();
        for (index, music) in [end, end + 100_000_000, end + 200_000_000].into_iter().enumerate() {
            gameplay::update(&mut state, 0.1, GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot { music_nanos: music,
                    has_music_mapping: true, valid_at_host_nanos: 2_000_000_000 + index as u64 * 100_000_000,
                    ..Default::default() }, ..Default::default()
            }, || 2_000_000_000, &mut effects);
            let output = state.take_song_lua_output();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let offs: Vec<_> = output.source_broadcasts.iter().filter(|event| event.name == "EngineOff").collect();
            assert_eq!(offs.len(), usize::from(index == 1), "Off runs on LeaveGameplay, after one actor update");
            assert_eq!(state.gameplay.players_runtime.players[0].stage_failed, index >= 1,
                "the actual overlay Off writes shared Core failure after the exit decision");
            if let Some(off) = offs.first() {
                assert_eq!(off.music_seconds, f64::from((music as f64 / 1_000_000_000.0) as f32));
            }
        }
    }

    #[test]
    fn arrow_effects_reads_owned_style_without_extra_lua_queries() {
        use deadsync_song_lua::{SongLuaCompileContext, SongLuaNoteskinResolver,
            compile_song_lua_with_default_host};
        use std::{fs, sync::Arc};
        super::start_call_log(10_000).expect("observational logger");
        let dir = std::env::temp_dir().join(format!("deadsync-arrow-style-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("script directory");
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
local ps = GAMESTATE:GetPlayerState(0)
assert(GAMESTATE:GetCurrentStyle():GetName() == 'single')
assert(ArrowEffects.GetXPos(ps, 1, 0) == -96)
GAMESTATE:SetCurrentStyle('double')
assert(ArrowEffects.GetXPos(ps, 8, 0) == 224)
return Def.Actor {}
"#).expect("style control");
        let mut context = SongLuaCompileContext::new(&dir, "owned style");
        context.live = true;
        compile_song_lua_with_default_host::<(), (), _>(&entry, &context,
            SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None, |_, _| None).expect("native column positions after style mutation");
        let events = super::CALL_EVENTS.with_borrow(|capture| capture.events.clone());
        let queries: Vec<_> = events.iter().filter(|event|
            event["operation"] == "GameState.GetCurrentStyle").collect();
        assert_eq!(queries.len(), 1, "internal engine reads must not emit Lua queries: {events:?}");
        assert_eq!(queries[0]["line"], 0, "native constructor query line");
        assert_eq!(queries[0]["phase"], "construction");
        assert!(queries[0]["source"].as_str().expect("authored query source")
            .ends_with("default.lua"));
        assert_eq!(queries[0]["results"][0]["name"], "single");
    }

    fn dynamic_command_observations(enabled: bool) -> Vec<serde_json::Value> {
        use deadsync_song_lua::{SongLuaCompileContext, SongLuaLiveFrame,
            SongLuaNoteskinResolver, compile_song_lua_with_default_host};
        use std::{fs, sync::Arc};
        super::start_call_log(10_000).expect("observational logger");
        let dir = std::env::temp_dir().join(format!("deadsync-dynamic-trace-{}-{enabled}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("default.lua");
        fs::write(&entry, r#"
return Def.Actor {
    InitCommand = function(self)
        self:addcommand('Set', function(owner)
            GAMESTATE:GetSongBeat()
            owner:playcommand('Declared')
        end)
        self:queuecommand('Set')
    end,
    DeclaredCommand = function(self) GAMESTATE:GetSongBeat() end,
}
"#).unwrap();
        let mut context = SongLuaCompileContext::new(&dir, "dynamic observer");
        context.live = true;
        context.trace_dynamic_commands = enabled;
        let mut compiled = compile_song_lua_with_default_host::<(), (), _>(
            &entry, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).unwrap();
        let output = compiled.live_scene.as_mut().unwrap().advance(SongLuaLiveFrame {
            music_seconds: 0.1, beat: 0.2, screen_seconds: 0.1,
            delta_seconds: 0.1, ..Default::default()
        }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        super::CALL_EVENTS.with_borrow(|capture| capture.events.clone())
    }

    #[test]
    fn dynamically_added_commands_keep_native_observer_provenance() {
        let events = dynamic_command_observations(false);
        assert!(!events.iter().any(|event| event["command"] == "SetCommand"), "{events:?}");
        let queries: Vec<_> = events.iter().filter(|event| event["operation"] == "GameState.GetSongBeat").collect();
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0]["phase"], "construction");
        assert_eq!(queries[1]["phase"], "callback");
        assert_eq!(events.iter().filter(|event| event["command"] == "DeclaredCommand").count(), 2);
    }

    #[test]
    fn dynamically_added_commands_observe_enabled_capture_profile() {
        let events = dynamic_command_observations(true);
        assert_eq!(events.iter().filter(|event| event["command"] == "SetCommand").count(),
            2, "dynamic command boundaries must observe actual dispatch: {events:?}");
        let queries: Vec<_> = events.iter().filter(|event| event["operation"] == "GameState.GetSongBeat").collect();
        assert_eq!(queries.len(), 2);
        assert!(queries.iter().all(|event| event["phase"] == "callback"));
        assert_eq!(events.iter().filter(|event| event["command"] == "DeclaredCommand").count(), 2);
    }

    #[test]
    fn native_multivertex_texture_getter_preserves_runtime_and_observation_scope() {
        use deadsync_song_lua::{
            SongLuaCompileContext, SongLuaNoteskinResolver, compile_song_lua_with_default_host,
        };
        use std::{fs, sync::Arc};
        super::start_call_log(250_000).expect("install observational logger");
        let dir = std::env::temp_dir()
            .join(format!("deadsync-multivertex-texture-trace-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("texture trace test directory");
        let entry = dir.join("texture-classes.lua");
        fs::write(&entry, r#"
local quad, sprite, graph, target, loaded
return Def.ActorFrame {
    OnCommand=function()
        local texture = assert(quad:GetTexture())
        assert(texture:GetTextureWidth() == 1 and texture:GetTextureHeight() == 1)
        assert(sprite:GetTexture() == nil)
        assert(target:GetTexture() == nil)
        local initial = assert(graph:GetTexture(), 'AMV must load its native default texture')
        assert(initial:GetPath() == '' and initial:GetNumFrames() == 1)
        assert(initial:GetTextureWidth() == 1 and initial:GetTextureHeight() == 1)
        local image = assert(loaded:GetTexture())
        graph:SetTexture(image)
        local graph_texture = assert(graph:GetTexture(), 'AMV texture getter must still execute')
        assert(graph_texture:GetPath() == image:GetPath())
        assert(graph_texture:GetTextureWidth() == image:GetTextureWidth())
        assert(graph_texture:GetTextureHeight() == image:GetTextureHeight())
        graph:SetTexture(texture)
        local restored = assert(graph:GetTexture())
        assert(restored:GetPath() == '' and restored:GetNumFrames() == 1)
        assert(restored:GetTextureWidth() == 1 and restored:GetTextureHeight() == 1)
    end,
    Def.Quad {InitCommand=function(self) quad=self end},
    Def.Sprite {InitCommand=function(self) sprite=self end},
    Def.ActorMultiVertex {InitCommand=function(self) graph=self end},
    Def.ActorFrameTexture {InitCommand=function(self) target=self end},
    Def.Sprite {Texture='__songlua_theme_path/G/_Judgments/Love 2x6.png',
        InitCommand=function(self) loaded=self end},
}
"#).expect("write texture trace control");
        let mut context = SongLuaCompileContext::new(&dir, "native texture class observations");
        context.live = true;
        let compiled = compile_song_lua_with_default_host::<(), (), _>(
            &entry, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None,
        ).expect("compile real texture getter control");
        assert!(compiled.live_scene.is_some());
        let textures = super::CALL_EVENTS.with_borrow(|capture| capture.events.iter()
            .filter(|event| event["operation"].as_str().is_some_and(|name| name.ends_with(".GetTexture")))
            .cloned().collect::<Vec<_>>());
        assert_eq!(textures.iter().map(|event| event["operation"].as_str().unwrap())
            .collect::<Vec<_>>(),
            ["Sprite.GetTexture", "Sprite.GetTexture", "ActorFrameTexture.GetTexture", "Sprite.GetTexture"],
            "AMV GetTexture must not impersonate the compared Sprite getter: {textures:?}");
        assert_eq!(textures[0]["results"], serde_json::json!([{"type":"RageTexture"}]));
        assert_eq!(textures[1]["results"], serde_json::json!([{"type":"nil"}]));
        assert_eq!(textures[2]["results"], serde_json::json!([{"type":"nil"}]));
        assert_eq!(textures[3]["results"], serde_json::json!([{"type":"RageTexture"}]));
        for event in textures {
            assert_eq!(event["phase"], "callback");
            assert!(event["source"].as_str().unwrap().ends_with("texture-classes.lua"));
        }
    }

    #[test]
    fn actor_and_query_trace_keep_lua_callers() {
        use super::start_call_log;
        use deadsync_song_lua::{
            SongLuaCompileContext, SongLuaDifficulty, SongLuaNoteskinResolver, SongLuaStep,
            compile_song_lua_with_default_host,
        };
        use std::{fs, sync::Arc};
        start_call_log(250_000).expect("install observational logger");
        let dir =
            std::env::temp_dir().join(format!("deadsync-actor-call-trace-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("trace test directory");
        let entry = dir.join("caller.lua");
        let source = "local song = GAMESTATE:GetCurrentSong()\n\
            local steps = song:GetStepsByStepsType('StepsType_Dance_Single')\n\
            assert(#steps == 2 and GAMESTATE:GetCurrentSteps(0) == steps[2])\n\
            assert(steps[1]:GetDifficulty() == 'Difficulty_Easy')\n\
            assert(GAMESTATE:IsPlayerEnabled(0))\n\
            GAMESTATE:SetCurrentSteps(0, steps[1])\n\
            assert(GAMESTATE:GetCurrentSteps(0) == steps[1])\n\
            return Def.ActorFrame {\n\
            InitCommand=function(self) self:sleep(0.125):queuecommand('Done')\n\
             self:linear(0.25):diffuse({0.25,0.5,0.75,1}):zoomto(320,240);self:SetUpdateFunction(function(owner) owner:aux(0) end) end,\n\
             Def.ActorProxy{InitCommand=function(self) self:SetTarget(self:GetParent()) end},\n\
             Def.Sprite{Texture='__songlua_theme_path/G/_Judgments/Love 2x6.png',InitCommand=function(self) self:loop(false):rate(1.25);self:GetTexture() end},\n\
            }";
        fs::write(&entry, source).expect("trace control");
        let mut context = SongLuaCompileContext::new(&dir, "class dispatch trace");
        context.live = true;
        context.song_steps = [SongLuaDifficulty::Easy, SongLuaDifficulty::Challenge]
            .into_iter()
            .map(|difficulty| SongLuaStep {
                steps_type: "StepsType_Dance_Single".into(),
                difficulty,
                description: format!("{difficulty:?}"),
                chart_name: String::new(),
                credit: String::new(),
                meter: 11,
                music_file: String::new(),
                possible_grade_points: 100,
            })
            .collect();
        context.player_step_indices[0] = Some(1);
        let compiled = compile_song_lua_with_default_host::<(), (), _>(
            &entry,
            &context,
            SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None,
            |_, _| None,
        )
        .expect("compile real live actor");
        assert!(compiled.live_scene.is_some());
        let events = super::CALL_EVENTS.with_borrow(|capture| capture.events.clone());
        let calls: Vec<_> = events
            .iter()
            .filter(|event| {
                matches!(
                    event["operation"].as_str(),
                    Some("Actor.sleep" | "Actor.queuecommand")
                )
            })
            .collect();
        assert_eq!(calls.len(), 2, "{events:?}");
        for event in &calls {
            assert!(
                event["source"]
                    .as_str()
                    .expect("Lua source")
                    .ends_with("caller.lua")
            );
            assert_eq!(event["line"], 9);
            assert_eq!(event["phase"], "callback");
            assert_eq!(event["tween_time_left"], 0.125);
        }
        assert_eq!(calls[0]["operation"], "Actor.sleep");
        assert_eq!(calls[0]["args"][0], 0.125);
        assert_eq!(calls[1]["operation"], "Actor.queuecommand");
        assert_eq!(calls[1]["args"][0], "Done");
        let color = events.iter().find(|event| event["operation"] == "Actor.diffuse")
            .expect("captured color mutation");
        assert!(super::same_value(&color["args"], &serde_json::json!([[0.25, 0.5, 0.75, 1]])));
        assert_eq!(color["line"], 9);
        assert_eq!(color["phase"], "callback");
        assert_eq!(color["tween_time_left"], 0.375);
        let update = events.iter().find(|event| event["operation"] == "ActorFrame.SetUpdateFunction")
            .expect("captured update callback installation");
        assert_eq!(update["args"][0]["type"], "function");
        assert!(update["args"][0]["source"].as_str().expect("callback source").ends_with("caller.lua"));
        assert_eq!(update["args"][0]["line"], 10);
        assert_eq!(update["args"][0]["last_line"], 10);
        let target = events.iter().find(|event| event["operation"] == "ActorProxy.SetTarget")
            .expect("captured proxy target");
        let parent = events.iter().find(|event| event["operation"] == "Actor.GetParent")
            .expect("captured actual parent");
        assert_eq!(target["args"][0], parent["results"][0]);
        let texture = events.iter().find(|event| event["operation"] == "Sprite.GetTexture")
            .expect("captured texture object");
        assert_eq!(texture["results"], serde_json::json!([{"type":"RageTexture"}]));
        assert_eq!(events.iter().find(|event| event["operation"] == "Sprite.loop")
            .expect("captured movie loop")["args"], serde_json::json!([false]));
        assert_eq!(events.iter().find(|event| event["operation"] == "Sprite.rate")
            .expect("captured movie rate")["args"], serde_json::json!([1.25]));
        let construction: Vec<_> = events
            .iter()
            .filter(|event| event["phase"] == "construction")
            .collect();
        assert_eq!(construction.len(), 7, "{events:?}");
        assert_eq!(
            construction
                .iter()
                .map(|event| event["operation"].as_str().expect("operation"))
                .collect::<Vec<_>>(),
            [
                "GameState.GetCurrentSong",
                "Song.GetStepsByStepsType",
                "GameState.GetCurrentSteps",
                "Steps.GetDifficulty",
                "GameState.IsPlayerEnabled",
                "GameState.SetCurrentSteps",
                "GameState.GetCurrentSteps"
            ]
        );
        assert!(construction.iter().all(|event| event["line"] == 0));
        assert_eq!(construction[1]["results"][0][0]["index"], 1);
        assert_eq!(construction[1]["results"][0][1]["index"], 2);
        assert_eq!(construction[2]["results"][0]["index"], 2);
        assert_eq!(construction[6]["results"][0]["index"], 1);
        drop(events);
        start_call_log(250_000).expect("reset observations for display and update control");
        let entry = dir.join("display-caller.lua");
        fs::write(
            &entry,
            r#"
local style = GAMESTATE:GetCurrentStyle()
assert(style:GetName() == 'single')
assert(GAMESTATE:GetCurrentSteps(0):GetStepsType() == 'StepsType_Dance_Single')
assert(DISPLAY:SupportsRenderToTexture() == false)
assert(PREFSMAN:GetPreference('DisplayWidth') == 0)
assert(PREFSMAN:GetPreference('DisplayHeight') == 0)
local state = GAMESTATE:GetPlayerState(1)
local options = state:GetPlayerOptions('ModsLevel_Song')
local amount, approach = options:Reverse()
assert(amount == 0 and approach == 1)
return Def.ActorFrame{Name='Control', InitCommand=function(self)
    self:x(12):y(24):SetUpdateFunction(function(self)
        local beat = GAMESTATE:GetSongBeat()
        assert(beat == 0 or beat == 0.25)
        assert(self:GetName() == 'Control')
        local screen = SCREENMAN:GetTopScreen()
        local player = screen:GetChild('PlayerP1')
        assert(player.GetChild == ActorFrame.GetChild)
        assert(player:GetChild('Judgment'):GetName() == 'Judgment')
        assert(screen:GetChild('PlayerP2') == nil)
        local count = screen:GetNumChildren()
        for _,name in ipairs({'ScoreP2','StepsDisplayP2','BPMDisplay','LifeFrame',
                'ScoreFrame','Lyrics','LifeMeter','UnknownChild123'}) do
            assert(screen:GetChild(name) == nil, name)
            assert(screen:GetChildren()[name] == nil, name)
        end
        for _,name in ipairs({'ScoreP1','StepsDisplayP1'}) do
            assert(screen:GetChild(name):GetName() == name)
        end
        assert(screen:GetNumChildren() == count)
    end)
end}
"#,
        )
        .expect("display and callback control");
        context.display_width = 0.0;
        context.display_height = 0.0;
        context.render_to_texture = false;
        context.players[1].enabled = false;
        let mut compiled = compile_song_lua_with_default_host::<(), (), _>(
            &entry,
            &context,
            SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None,
            |_, _| None,
        )
        .expect("compile actual getters and update function");
        let output = compiled
            .live_scene
            .as_mut()
            .expect("owned scene")
            .advance(deadsync_song_lua::SongLuaLiveFrame {
                beat: 0.25,
                music_seconds: 0.25,
                screen_seconds: 0.25,
                delta_seconds: 0.01,
                uptime_seconds: None,
                gameplay_outro: false,
                chart_attacks: None,
                players: std::array::from_fn(|_| deadsync_song_lua::SongLuaLivePlayer::default()),
            })
            .expect("advance update callback");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let events = super::CALL_EVENTS.with_borrow(|capture| capture.events.clone());
        for (operation, results) in [
            (
                "RageDisplay.SupportsRenderToTexture",
                serde_json::json!([false]),
            ),
            ("Style.GetName", serde_json::json!(["single"])),
            ("PlayerOptions.Reverse", serde_json::json!([0, 1])),
        ] {
            let event = events
                .iter()
                .find(|event| event["operation"] == operation)
                .expect("observed constructor getter");
            assert_eq!(event["kind"], "query");
            assert_eq!(event["phase"], "construction");
            assert!(super::same_value(&event["results"], &results));
        }
        let beats: Vec<_> = events
            .iter()
            .filter(|event| event["operation"] == "GameState.GetSongBeat")
            .collect();
        assert!(beats.iter().all(|event| event["phase"] == "callback"));
        assert_eq!(
            beats
                .iter()
                .map(|event| event["results"].clone())
                .collect::<Vec<_>>(),
            [serde_json::json!([0]), serde_json::json!([0.25])]
        );
        let children: Vec<_> = events
            .iter()
            .filter(|event| event["operation"] == "ActorFrame.GetChild")
            .collect();
        assert_eq!(children.len(), 26, "{events:?}");
        assert_eq!(
            children[1]["results"][0]["path"],
            "ScreenGameplay/PlayerP1/Judgment"
        );
        assert_eq!(children[2]["results"], serde_json::json!([{"type":"nil"}]));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event["operation"].as_str(), Some("Actor.x" | "Actor.y")))
                .count(),
            2
        );
    }

    #[test]
    fn named_query_handles_keep_actor_identity() {
        use serde_json::json;
        use std::collections::BTreeMap;
        let actor = |id: &str| json!({"actor":id, "name":"Judgment", "path":"ScreenGameplay/PlayerP1/Judgment"});
        let mut native = json!([actor("external-0016"), actor("external-0016")]);
        let mut actual = json!([actor("0x1234"), actor("0x1234")]);
        super::normalize_query_handles(&mut native, &mut BTreeMap::new(), 1);
        super::normalize_query_handles(&mut actual, &mut BTreeMap::new(), 1);
        assert!(super::same_value(&native, &actual));
        let mut wrong_alias = json!([actor("0x1234"), actor("0x5678")]);
        super::normalize_query_handles(&mut wrong_alias, &mut BTreeMap::new(), 1);
        assert!(!super::same_value(&native, &wrong_alias));
        actual[0]["path"] = json!("ScreenGameplay/PlayerP2/Judgment");
        assert!(!super::same_value(&native, &actual));
        actual[0] = native[0].clone();
        actual[0]["name"] = json!("Combo");
        assert!(!super::same_value(&native, &actual));
        let mut unnamed = json!({"actor":"external-0099"});
        super::normalize_query_handles(&mut unnamed, &mut BTreeMap::new(), 1);
        assert_eq!(unnamed, json!({"actor":"external-0099"}));
    }

    #[test]
    fn query_handle_numbering_remains_global_across_scene_epochs() {
        let mut handles = std::collections::BTreeMap::new();
        for (scene, address, expected) in [(1, "a", 1), (2, "a", 2),
            (1, "b", 3), (3, "a", 4), (2, "a", 2), (1, "a", 1)] {
            let mut value = serde_json::json!({"actor":address,"name":"Child","path":"Screen/Child"});
            super::normalize_query_handles(&mut value, &mut handles, scene);
            assert_eq!(value["actor"], expected);
            assert_eq!(value["name"], "Child");
            assert_eq!(value["path"], "Screen/Child");
        }
    }

    #[test]
    fn call_source_normalization_preserves_original_path_rules() {
        for directory in ["/tmp/song/", "C:/Songs/Song/", "/songs/音楽/"] {
            for source in [format!("@{directory}default.lua"), format!("prefix{directory}default.lua"),
                format!("@{directory}nested/{directory}default.lua"),
                format!("@{directory}default.lua").replace('/', "\\"),
                "song:/default.lua".into(), "song:/nested\\default.lua".into(),
                "@theme:/default.lua".into(), "".into()] {
                let normalized = source.replace('\\', "/");
                let expected = normalized.split_once(directory)
                    .map(|(_, relative)| format!("song:/{relative}"))
                    .or_else(|| normalized.starts_with("song:/").then(|| normalized.clone()));
                let mut event = serde_json::json!({"source":source,"other":"unchanged"});
                assert_eq!(super::normalize_call_source(&mut event, directory), expected.is_some());
                if let Some(expected) = expected { assert_eq!(event["source"], expected); }
                assert_eq!(event["other"], "unchanged");
            }
        }
        for source in [serde_json::Value::Null, serde_json::json!(42)] {
            let mut event = serde_json::json!({"source":source});
            assert!(!super::normalize_call_source(&mut event, "/tmp/song/"));
        }
    }

    #[test]
    fn coverage_tracks_observed_apis_and_constructor_evidence() {
        use super::{Scope, coverage};
        use serde_json::json;
        let mut trace = json!({"capabilities":{"native_call_sources":true},
            "actor_definitions":[{"class":"ActorFrame"}],
            "events":[{"operation":"Actor.sleep","source":"song:/control.lua",
                "caller":{"source":"song:/control.lua"}},
                {"operation":"PlayerStageStats.SetScore","source":"song:/control.lua",
                    "caller":{"source":"@Themes/Simply Love/out.lua"}}]});
        let calls = json!({"compared":true});
        let value = coverage(&trace, Scope::Full, &calls, 4, 4);
        assert_eq!(
            value["pending"],
            json!(["native constructor query results are not captured"])
        );
        assert_eq!(value["uncompared_song_operations"], json!([]));
        assert_eq!(
            value["nested_theme_operations"],
            json!(["PlayerStageStats.SetScore"])
        );
        assert!(
            !value["global_pending"]
                .as_array()
                .expect("global gaps")
                .is_empty()
        );
        trace["capabilities"]["native_constructor_queries"] = json!(true);
        assert_eq!(
            coverage(&trace, Scope::Full, &calls, 4, 4)["pending"],
            json!([])
        );
        trace["events"][0]["operation"] = json!("PlayerStageStats.GetAliveSeconds");
        assert_eq!(
            coverage(&trace, Scope::Full, &calls, 4, 4)["uncompared_song_operations"],
            json!(["PlayerStageStats.GetAliveSeconds"])
        );
    }

    #[test]
    fn passing_archives_advance_with_recorded_coverage_gaps() {
        use super::{Scope, archive_passes};
        use serde_json::json;
        let report = json!({"native_frames":10,"replayed_frames":10,
            "mismatches":[],"errors":[],"whole_song_parity_proven":false,
            "pending_coverage":["native drawable state/resources: Sprite"]});
        assert!(archive_passes(Scope::Full, &report));
        assert!(!archive_passes(Scope::ThroughSecond(1.0), &report));
        for (field, value) in [
            ("replayed_frames", json!(9)),
            ("native_frames", json!(0)),
            ("mismatches", json!([{"field":"command_query_event"}])),
            ("errors", json!(["callback failure"])),
            ("errors", json!(null)),
        ] {
            let mut failed = report.clone();
            failed[field] = value;
            assert!(!archive_passes(Scope::Full, &failed), "accepted {field}");
        }
    }

    #[test]
    fn partial_frames_and_drawable_resources_remain_pending() {
        use super::{Scope, coverage};
        use serde_json::json;
        let trace = json!({"capabilities":{"native_call_sources":true,"native_constructor_queries":true},
            "events":[],"actor_definitions":[{"class":"Sprite"}]});
        for (scope, replayed) in [
            (Scope::FirstQuery, 3),
            (Scope::ThroughSecond(0.5), 3),
            (Scope::ThroughSecond(1.0), 10),
        ] {
            let value = coverage(&trace, scope, &json!({"compared":true}), replayed, 10);
            assert_eq!(
                value["pending"],
                json!([
                    "complete song frame replay",
                    "native drawable state/resources: Sprite"
                ])
            );
        }
        assert_eq!(
            coverage(&trace, Scope::Full, &json!({"compared":false}), 10, 10)["pending"],
            json!([
                "native command/query scene identities",
                "native drawable state/resources: Sprite"
            ])
        );
    }

    #[test]
    fn call_arguments_compare_native_numbers_without_losing_value_checks() {
        use super::same_value;
        use serde_json::json;
        assert!(same_value(&json!([1]), &json!([1.0])));
        assert!(!same_value(&json!([1]), &json!([2.0])));
        assert!(!same_value(&json!(["CheckEnd"]), &json!(["Finished"])));
        assert!(!same_value(&json!([1]), &json!([1, 2])));
        assert!(same_value(
            &json!({"type":"number","value":"infinity"}),
            &json!({"type":"number","value":"infinity"})
        ));
    }

    #[test]
    fn ending_stage_observers_follow_final_frame_after_music_rewind() {
        use serde_json::json;
        let frames=vec![json!({"scene":1,"seconds":10.0}),
            json!({"scene":1,"seconds":10.05}),
            json!({"scene":1,"seconds":10.02})];
        let mut samples=vec![json!({"phase":"progress","scene":1,"seconds":10.0}),
            json!({"phase":"screen-end","scene":1,"seconds":10.02}),
            json!({"phase":"gameplay-exit","scene":1,"seconds":10.02})];
        let expected=[(0,0),(1,2),(2,2)].into_iter().collect();
        assert_eq!(super::native_progress_observer_frames(&samples,&frames).expect("recorded ending phases"),
            expected, "ending observations must wait for their matching native frame, retaining both samples");
        samples[1]["players"]=json!([{"actual":9999,"failed":true}]);
        assert_eq!(super::native_progress_observer_frames(&samples,&frames).expect("independent ending phase"),expected);
        samples[1]["seconds"]=json!(10.03);
        assert!(!super::native_progress_observer_frames(&samples,&frames).expect("unmatched ending clock remains pending").contains_key(&1));
    }

    #[test]
    fn progress_observers_follow_frame_order_when_music_rewinds() {
        use serde_json::json;
        let frames = vec![
            json!({"scene":1,"seconds":20.0}),
            json!({"scene":1,"seconds":21.05}),
            json!({"scene":1,"seconds":20.978}),
            json!({"scene":1,"seconds":21.0}),
            json!({"scene":2,"seconds":20.978}),
        ];
        let mut samples = vec![
            json!({"phase":"progress","scene":1,"seconds":20.0}),
            json!({"phase":"progress","scene":1,"seconds":20.978}),
            json!({"phase":"progress","scene":1,"seconds":21.0}),
            json!({"phase":"progress","scene":2,"seconds":20.978}),
        ];
        let expected = [(0,0),(1,2),(2,3),(3,4)].into_iter().collect();
        assert_eq!(super::native_progress_observer_frames(&samples,&frames).expect("observer clocks"), expected);
        samples[1]["players"] = json!([{"actual":9999}]);
        assert_eq!(super::native_progress_observer_frames(&samples,&frames).expect("independent observer phase"), expected);
        samples[3]["seconds"] = json!(123.0);
        assert!(!super::native_progress_observer_frames(&samples,&frames).expect("unmatched clock remains pending").contains_key(&3));
    }

    #[test]
    fn instantiated_background_effect_maps_its_authored_child() {
        use serde_json::json;
        let trace = json!({
            "capabilities":{"native_actor_creation":true}, "roots":["effect","fg"],
            "actor_definitions":[
                {"id":"effect","class":"ActorFrame","source":"@/BackgroundEffects/UpperLeft.lua","children":[]},
                {"id":"entry","class":"ActorFrame","source":"@/BackgroundEffects/UpperLeft.lua","children":[]},
                {"id":"quad","class":"Quad","source":"song:/bg/default.lua","children":[]},
                {"id":"sprite","class":"Sprite","source":"song:/bg/default.lua","children":[]},
                {"id":"fg","class":"ActorFrame","source":"song:/fg/default.lua","children":[]}],
            "runtime_actors":[
                {"id":"effect-a","definition_id":"effect"},
                {"id":"entry-a","definition_id":"entry"},
                {"id":"quad-a","definition_id":"quad"},
                {"id":"sprite-a","definition_id":"sprite"},
                {"id":"fg-a","definition_id":"fg"},
                {"id":"fg-parent","path":"SongForeground"}],
            "events":[], "actor_creation_inputs":[
                {"actor":"effect-a","definition_id":"effect","pointer_hex":"50","ordinal":1,"scene":1,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"},
                {"actor":"entry-a","definition_id":"entry","parent":"effect-a","pointer_hex":"40","ordinal":2,"scene":1,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"},
                {"actor":"quad-a","definition_id":"quad","parent":"entry-a","pointer_hex":"30","ordinal":3,"scene":1,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"},
                {"actor":"sprite-a","definition_id":"sprite","parent":"entry-a","pointer_hex":"20","ordinal":4,"scene":1,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"},
                {"actor":"fg-a","definition_id":"fg","parent":"fg-parent","pointer_hex":"10","ordinal":5,"scene":1,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"}]
        });
        let expected = vec![("1".into(),64),("1/1".into(),48),("1/2".into(),32),("2".into(),16)];
        assert_eq!(super::native_creation_order_for_entries(&trace,1,1)
            .expect("instantiated effect ownership"), expected);
        for effect in ["Centered", "StretchNoLoop", "StretchRewind"] {
            let mut wrapped = trace.clone();
            for index in [0, 1] {
                wrapped["actor_definitions"][index]["source"] = json!(format!("@/BackgroundEffects/{effect}.lua"));
            }
            assert_eq!(super::native_creation_order_for_entries(&wrapped,1,1)
                .expect("native background effect retains its authored child"), expected);
        }
        let mut background_only = trace.clone();
        background_only["actor_creation_inputs"].as_array_mut().unwrap().pop();
        background_only["runtime_actors"].as_array_mut().unwrap().remove(4);
        assert_eq!(super::native_creation_order_for_entries(&background_only,1,1)
            .expect("background-only scene"), expected[..3]);
        let mut changed_results = trace.clone();
        changed_results["events"] = json!([{"operation":"message","detail":{"results":["unrelated"]}}]);
        assert_eq!(super::native_creation_order_for_entries(&changed_results,1,1)
            .expect("factory inputs remain independent of observed results"), expected);
        for (path,value) in [
            ("/actor_creation_inputs/0/pointer_hex",json!("40")),
            ("/actor_creation_inputs/1/parent",json!(null)),
            ("/actor_creation_inputs/1/scene",json!(2)),
            ("/actor_creation_inputs/2/provenance",json!("expected-handler-order")),
            ("/actor_definitions/0/source",json!("song:/BackgroundEffects/StretchNoLoop.lua")),
        ] {
            let mut invalid = trace.clone();
            *invalid.pointer_mut(path).expect("input field") = value;
            assert!(super::native_creation_order_for_entries(&invalid,1,1).is_err(), "accepted invalid {path}");
        }
    }

    #[test]
    fn creation_identities_keep_foreground_when_background_factory_is_unobserved() {
        let trace = json!({
            "capabilities":{"native_actor_creation":true}, "roots":["fg"],
            "actor_definitions":[{"id":"fg","children":[]}], "events":[],
            "runtime_actors":[{"id":"fg-instance","definition_id":"fg"},
                {"id":"parent","path":"SongForeground"}],
            "actor_creation_inputs":[{"actor":"fg-instance","definition_id":"fg","parent":"parent",
                "pointer_hex":"20","ordinal":1,"scene":1,"before_seq":1,
                "provenance":"native-ActorUtil-before-LoadFromNode"}]
        });
        let (order, coverage) = super::native_creation_order_with_coverage_for_entries(&trace,1,1)
            .expect("native failed background has no observed factory allocation");
        assert_eq!(order, vec![("2".into(),32)]);
        assert_eq!(coverage.len(), 1);
        assert!(coverage[0].contains("foreground ranks retained"));
    }

    #[test]
    fn creation_identities_use_allocations_and_validate_ownership() {
        use serde_json::json;
        let trace = json!({
            "capabilities": {"native_actor_creation": true}, "roots": ["root"],
            "actor_definitions": [
                {"id":"root", "children":[{"layer_index":1,"definition_id":"child"}]},
                {"id":"child", "children":[]}],
            "runtime_actors":[{"id":"a", "definition_id":"root"},
                {"id":"b", "definition_id":"child"}],
            "events": [],
            "actor_creation_inputs":[
                {"actor":"a","definition_id":"root","pointer_hex":"20","ordinal":1,
                    "scene":1,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"},
                {"actor":"b","definition_id":"child","parent":"a","pointer_hex":"10","ordinal":2,
                    "scene":1,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"}]
        });
        let expected = vec![("1".into(), 32), ("1/1".into(), 16)];
        assert_eq!(super::native_creation_order(&trace).expect("native identities"), expected);
        let mut different_results = trace.clone();
        different_results["events"] = json!([{"operation":"command.begin","actor":"b"},
            {"operation":"command.begin","actor":"a"}]);
        assert_eq!(super::native_creation_order(&different_results).expect("neutral inputs"), expected);
        // Def.* snapshots precede later table appends, and some declarations
        // are never instantiated. Factory parents still describe the tree.
        let mut appended = trace.clone();
        appended["actor_definitions"][0]["children"] = json!([]);
        appended["roots"] = json!(["root", "child", "unused"]);
        appended["actor_definitions"].as_array_mut().unwrap().push(json!({"id":"unused", "children":[]}));
        assert_eq!(super::native_creation_order(&appended).expect("final native tree"), expected);
        // Background::LoadFromNode creates theme effect children directly on
        // an external frame. Those allocations are outside the song's owned
        // Foreground tree and must not shift its entry paths.
        let mut background = trace.clone();
        background["actor_definitions"].as_array_mut().unwrap().push(json!({"id":"bg", "children":[]}));
        background["runtime_actors"].as_array_mut().unwrap().extend([
            json!({"id":"bg-actor", "definition_id":"bg"}),
            json!({"id":"bg-parent", "name":"", "path":"bg-parent"}),
            json!({"id":"fg-parent", "name":"SongForeground", "path":"SongForeground"})]);
        background["actor_creation_inputs"][0]["parent"] = json!("fg-parent");
        background["actor_creation_inputs"][0]["ordinal"] = json!(2);
        background["actor_creation_inputs"][1]["ordinal"] = json!(3);
        background["actor_creation_inputs"].as_array_mut().unwrap().insert(0,
            json!({"actor":"bg-actor","definition_id":"bg","parent":"bg-parent",
                "pointer_hex":"50","ordinal":1,"scene":1,"before_seq":1,
                "provenance":"native-ActorUtil-before-LoadFromNode"}));
        assert_eq!(super::native_creation_order(&background).expect("owned foreground inputs"), expected);
        assert_eq!(super::native_creation_order_for_entries(&background, 1, 1)
            .expect("background children and second entry foreground"),
            vec![("1/1".into(), 80), ("2".into(), 32), ("2/1".into(), 16)]);
        // A repeated foreground filename still creates an independent root
        // and subtree. Declaration IDs alone cannot identify those instances.
        let mut repeated = background.clone();
        repeated["runtime_actors"].as_array_mut().unwrap().extend([
            json!({"id":"repeat-root", "definition_id":"root"}),
            json!({"id":"repeat-child", "definition_id":"child"})]);
        repeated["actor_creation_inputs"].as_array_mut().unwrap().extend([
            json!({"actor":"repeat-root","definition_id":"root","parent":"fg-parent",
                "pointer_hex":"40","ordinal":4,"scene":1,"before_seq":1,
                "provenance":"native-ActorUtil-before-LoadFromNode"}),
            json!({"actor":"repeat-child","definition_id":"child","parent":"repeat-root",
                "pointer_hex":"30","ordinal":5,"scene":1,"before_seq":1,
                "provenance":"native-ActorUtil-before-LoadFromNode"})]);
        assert_eq!(super::native_creation_order_for_entries(&repeated, 1, 1)
            .expect("every repeated foreground owns its own prepared slot"),
            vec![("1/1".into(), 80), ("2".into(), 32), ("2/1".into(), 16),
                 ("3".into(), 64), ("3/1".into(), 48)]);
        repeated["events"] = json!([{"operation":"command.begin","actor":"repeat-child"},
            {"operation":"command.begin","actor":"b"}]);
        assert_eq!(super::native_creation_order_for_entries(&repeated, 1, 0)
            .expect("handler observations cannot seed the entry ownership"),
            vec![("1".into(), 32), ("1/1".into(), 16), ("2".into(), 64), ("2/1".into(), 48)]);
        let mut reload = trace.clone();
        reload["actor_definitions"].as_array_mut().unwrap().extend([
            json!({"id":"next-root", "children":[{"layer_index":1,"definition_id":"next-child"}]}),
            json!({"id":"next-child", "children":[]})]);
        reload["roots"] = json!(["root", "next-root"]);
        reload["runtime_actors"].as_array_mut().unwrap().extend([
            json!({"id":"c", "definition_id":"next-root"}),
            json!({"id":"d", "definition_id":"next-child"})]);
        reload["actor_creation_inputs"].as_array_mut().unwrap().extend([
            json!({"actor":"c","definition_id":"next-root","pointer_hex":"40","ordinal":3,
                "scene":2,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"}),
            json!({"actor":"d","definition_id":"next-child","parent":"c","pointer_hex":"30","ordinal":4,
                "scene":2,"before_seq":1,"provenance":"native-ActorUtil-before-LoadFromNode"})]);
        assert_eq!(super::native_creation_order(&reload).expect("initial epoch"), expected);
        assert_eq!(super::native_creation_order_for_scene(&reload, 2).expect("replacement epoch"),
            vec![("1".into(), 64), ("1/1".into(), 48)]);
        assert!(super::native_creation_order(&json!({})).expect("older capture").is_empty());
        let mut empty_tree = trace.clone();
        for field in ["roots", "actor_definitions", "runtime_actors", "actor_creation_inputs"] {
            empty_tree[field] = json!([]);
        }
        assert!(super::native_creation_order(&empty_tree).expect("empty native actor branch").is_empty());
        empty_tree["runtime_actors"] = json!([{"id":"external-0001", "name":"PlayerP1"}]);
        assert!(super::native_creation_order(&empty_tree).expect("screen actors are external to the song tree").is_empty());
        empty_tree["runtime_actors"][0]["definition_id"] = json!("unknown");
        assert!(super::native_creation_order(&empty_tree).is_err());
        for (path, value) in [
            ("/actor_creation_inputs/1/parent", json!("unknown")),
            ("/actor_creation_inputs/1/pointer_hex", json!("20")),
            ("/actor_creation_inputs/1/scene", json!(2)),
            ("/actor_creation_inputs/1/ordinal", json!(1)),
            ("/actor_creation_inputs/1/before_seq", json!(2)),
            ("/actor_creation_inputs/1/provenance", json!("expected-handler-order")),
            ("/actor_creation_inputs", json!([])),
            ("/runtime_actors/1/definition_id", json!("unknown")),
            ("/actor_creation_inputs/1/parent", json!(null)),
        ] {
            let mut invalid = trace.clone();
            *invalid.pointer_mut(path).expect("input field") = value;
            assert!(super::native_creation_order(&invalid).is_err(), "accepted invalid {path}");
        }
    }
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("missing finite native {key}"))
}

fn broadcast_event(event: &deadsync_song_lua::SongLuaLiveBroadcast, simfile: &Path) -> Value {
    fn normalize(value: &mut Value, song_dir: &str) {
        match value {
            Value::String(value) => {
                if let Some((_, relative)) = value.split_once(song_dir) {
                    *value = format!("song:/{relative}");
                }
            }
            Value::Array(values) => values
                .iter_mut()
                .for_each(|value| normalize(value, song_dir)),
            Value::Object(values) => values
                .values_mut()
                .for_each(|value| normalize(value, song_dir)),
            _ => {}
        }
    }
    let mut args = vec![json!(event.name)];
    if let Ok(Some(params)) = &event.params {
        let mut params = params.clone();
        let song_dir = format!(
            "{}/",
            simfile
                .parent()
                .unwrap_or(Path::new("."))
                .to_string_lossy()
                .replace('\\', "/")
        );
        normalize(&mut params, &song_dir);
        args.push(params);
    }
    json!({"seconds":event.music_seconds,"beat":event.beat,"name":event.name,
        "args":args,"params_error":event.params.as_ref().err()})
}

fn native_fail_type(trace: &Value) -> Result<deadsync_gameplay::GameplayFailType, String> {
    use deadsync_gameplay::GameplayFailType;
    use deadsync_rules::{note, timing};
    let preferences = &trace["input"]["gameplay_preferences"];
    if trace["capabilities"]["native_gameplay_preferences"] != true
        || preferences["initialization"] != "theme-game-mode"
        || preferences["game_mode"] != "ITG"
        || preferences["mod_level"] != "ModsLevel_Preferred"
    {
        return Err(
            "native capture must initialize and record Simply Love ITG gameplay inputs".into(),
        );
    }
    if number(preferences, "timing_window_scale")? != 1.0
        || number(preferences, "timing_window_add")? as f32 != timing::TIMING_WINDOW_ADD_S
        || number(preferences, "max_input_latency_seconds")? != 0.0
        || number(preferences, "music_rate")? != 1.0
    {
        return Err(
            "nondefault native timing scale, addition, latency or rate remains pending".into(),
        );
    }
    let windows = &preferences["timing_windows"];
    let add = timing::TIMING_WINDOW_ADD_S;
    for (key, expected) in [
        ("W1", timing::BASE_W1_S),
        ("W2", timing::BASE_W2_S),
        ("W3", timing::BASE_W3_S),
        ("W4", timing::BASE_W4_S),
        ("W5", timing::BASE_W5_S),
        ("Mine", timing::BASE_MINE_S),
    ] {
        if number(windows, key)? as f32 != expected {
            return Err(format!(
                "native {key} timing does not match the independent gameplay rules"
            ));
        }
    }
    for (key, expected) in [
        ("Hold", note::TIMING_WINDOW_SECONDS_HOLD),
        ("Roll", note::TIMING_WINDOW_SECONDS_ROLL),
    ] {
        if number(windows, key)? as f32 + add != expected {
            return Err(format!(
                "native {key} timing does not match the independent gameplay rules"
            ));
        }
    }
    for key in ["Attack", "Checkpoint"] {
        if number(windows, key)? as f32 + add > note::TIMING_WINDOW_SECONDS_ROLL {
            return Err(format!(
                "native {key} extends the resolution window; this context remains pending"
            ));
        }
    }
    let players = preferences["players"].as_array().filter(|p| !p.is_empty() && p.len() <= 2)
        .ok_or("native gameplay inputs must contain one or two players")?;
    let player = &players[0];
    if players.iter().enumerate().any(|(index,p)| p["player"] != format!("PlayerNumber_P{}",index+1)
        || p["disabled_timing_windows"] != json!([]) || p["min_tns_to_hide_notes"] != "TapNoteScore_W3"
        || p["fail_setting"] != player["fail_setting"]) {
        return Err("native player timing or fail modifiers differ; this context remains pending".into());
    }
    match player["fail_setting"].as_str() {
        Some("FailType_Immediate") => Ok(GameplayFailType::Immediate),
        Some("FailType_ImmediateContinue") => Ok(GameplayFailType::ImmediateContinue),
        Some("FailType_EndOfSong") => Ok(GameplayFailType::EndOfSong),
        Some("FailType_Off") => Ok(GameplayFailType::Off),
        _ => Err("native preferred player fail setting is missing or unknown".into()),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Scope {
    Full,
    Entry,
    FirstQuery,
    ThroughSecond(f64),
}

pub(crate) fn validate_capture(trace: &Value) -> Result<(), String> {
    native_random_seed(trace)?;
    if trace["capabilities"]["native_random_seed"] == true
        && (!trace["random_reseeds"].is_array() || trace["dropped_random_reseeds"] != 0)
    {
        return Err("declared native Lua random inputs are incomplete".into());
    }
    if trace["oracle"] != "itgmania_song_lua_semantic_trace"
        || trace["capabilities"]["native_stage_statistics"] != true
        || trace["capabilities"]["native_machine_preferences"] != true
        || trace["capabilities"]["native_update_frames"] != true
        || trace["capabilities"]["native_screen_lifecycle"] != true
        || trace["dropped_update_frames"] != 0
        || trace["dropped_events"] != 0
        || trace["dropped_stage_samples"] != 0
        || (trace["completion"]["reason"] != "gameplay-exit"
            && !(trace["completion"]["reason"] == "beat-limit"
                && trace["capabilities"]["native_drawable_vertices"] == true))
        || (trace["completion"]["stage_committed"] != true
            && trace["capabilities"]["native_drawable_vertices"] != true)
        || trace["update_frame_phase"] != "gameplay-overlay-probe"
        || (trace["input"]["injected_inputs"] != false
            && trace["capabilities"]["native_input_events"] != true)
    {
        return Err("a complete executable capture with actual frame clocks is required".into());
    }
    if trace["capabilities"]["native_display_preference"] == true
        && !trace["input"]["preferences"]["display_aspect_ratio"].as_f64()
            .is_some_and(|value| value >= 0.0 && value <= f64::from(f32::MAX))
    {
        return Err("declared native display preference is missing or invalid".into());
    }
    if trace["capabilities"]["native_initial_player_options"] == true
        && !trace["input"]["gameplay_preferences"]["players"].as_array().is_some_and(|players| {
            !players.is_empty() && players.len() <= 2 && players.iter().all(|player| {
                matches!(player["player"].as_str(), Some("PlayerNumber_P1" | "PlayerNumber_P2"))
                    && player["options_string"].as_str().is_some()
            })
        })
    {
        return Err("declared native initial player options are incomplete".into());
    }
    for (field, sequence) in [("update_frames", "sequence"), ("events", "seq")] {
        let values = trace[field].as_array().ok_or_else(|| format!("native {field} are missing"))?;
        // A Lua foreground may return an empty actor (for example, a calendar
        // condition). Its complete run still has gameplay frames and stage
        // samples, but there are no authored commands to observe.
        if field == "update_frames" && values.is_empty() {
            return Err("native update_frames are empty".into());
        }
        if values
            .iter()
            .enumerate()
            .any(|(index, value)| value[sequence].as_u64() != Some(index as u64 + 1))
        {
            return Err(format!("native {field} sequence is incomplete"));
        }
    }
    if trace["events"]
        .as_array()
        .is_some_and(|events| events.iter().any(|event| event["kind"] == "error"))
    {
        return Err("native Lua errors invalidate the reference".into());
    }
    if trace["stage_samples"]
        .as_array()
        .is_none_or(|samples| samples.is_empty())
    {
        return Err("native stage observations are empty".into());
    }
    interactive::validate(trace)?;
    Ok(())
}

// ActorFrame::LoadChildrenFromNode instantiates children in XNode array order
// (XmlFileUtil::XNodeFromTableRecursive). The factory observes the final parent
// tree, including children appended after Def.* recorded a declaration. Only
// these neutral allocations determine identities, never callback results.
fn native_creation_order(trace: &Value) -> Result<Vec<(String, usize)>, String> {
    native_creation_order_for_scene(trace, 1)
}

fn native_creation_order_for_scene(trace: &Value, scene: u64) -> Result<Vec<(String, usize)>, String> {
    native_creation_order_for_entries(trace, scene, 0)
}

fn native_creation_order_for_entries(trace: &Value, scene: u64, background_entries: usize) -> Result<Vec<(String, usize)>, String> {
    native_creation_order_with_coverage_for_entries(trace, scene, background_entries)
        .map(|(order, _)| order)
}

fn native_creation_order_with_coverage_for_entries(trace: &Value, scene: u64, background_entries: usize) -> Result<(Vec<(String, usize)>, Vec<String>), String> {
    let mut pending_coverage = Vec::new();
    if trace["capabilities"]["native_actor_creation"] != true { return Ok((Vec::new(), Vec::new())); }
    if !trace["actor_creation_error"].is_null() {
        return Err("native actor creation observer failed".into());
    }
    let roots = trace["roots"].as_array().ok_or("missing native actor roots")?;
    let definitions = trace["actor_definitions"].as_array().ok_or("missing native definitions")?;
    if roots.is_empty() && definitions.is_empty()
        && trace["actor_creation_inputs"].as_array().is_some_and(Vec::is_empty)
        && trace["runtime_actors"].as_array().is_some_and(|actors|
            actors.iter().all(|actor| actor["definition_id"].is_null()))
    {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut defs = BTreeMap::new();
    for definition in definitions {
        let id = definition["id"].as_str().ok_or("missing native definition identity")?;
        if defs.insert(id, definition).is_some() { return Err("duplicate native definition identity".into()); }
    }
    let inputs = trace["actor_creation_inputs"].as_array().ok_or("missing native creation identities")?;
    let event_count = trace["events"].as_array().ok_or("missing native event bounds")?.len();
    let runtime = trace["runtime_actors"].as_array().ok_or("missing native runtime instances")?;
    let mut instances = BTreeMap::new();
    for record in runtime {
        if let Some(definition) = record["definition_id"].as_str() {
            let actor = record["id"].as_str().ok_or("invalid native runtime identity")?;
            if !defs.contains_key(definition) || instances.insert(actor, definition).is_some() {
                return Err("invalid native runtime definition identity".into());
            }
        }
    }
    if instances.len() != inputs.len() {
        return Err("native factory observations do not cover the complete actor tree".into());
    }
    // Foreground::LoadFromNode attaches each song Lua entry to its native
    // SongForeground parent. Theme BackgroundEffects may also be observed in
    // this capture, including wrappers whose children load without the wrapper.
    let foreground_roots = inputs.iter().filter(|input| input["scene"].as_u64() == Some(scene)
        && runtime.iter().any(|record| record["id"] == input["parent"]
            && record["definition_id"].is_null() && record["path"] == "SongForeground"))
        .map(|input| input["actor"].as_str().ok_or("invalid native foreground identity"))
        .collect::<Result<Vec<_>, _>>()?;
    // Foreground::LoadFromSong calls MakeActor for each change, even when
    // several changes load the same filename or declaration. Construction
    // input order identifies the independent prepared entry slots.
    let foreground_roots: BTreeMap<_, _> = foreground_roots.into_iter().enumerate()
        .map(|(index, actor)| (actor, (background_entries + index + 1).to_string()))
        .collect();
    let mut observed = BTreeSet::new();
    let mut paths = BTreeMap::new();
    let mut background_roots = BTreeMap::new();
    let mut background_effects = BTreeSet::new();
    if background_entries > 0 {
        // Older captures observe children loaded directly onto an external
        // background frame. A composed ActorDef can instead expose the full
        // instantiated UpperLeft/Centered effect and its LoadActor(File1)
        // child. That child is the authored entry; the effect is theme-owned.
        let mut background_owners = Vec::new();
        for input in inputs.iter().filter(|input| input["scene"].as_u64() == Some(scene)) {
            let parent = input["parent"].as_str();
            if parent.is_some_and(|parent| instances.contains_key(parent)) || runtime.iter().any(|record| record["id"] == input["parent"]
                && record["path"] == "SongForeground") { continue; }
            let definition = input["definition_id"].as_str().and_then(|id| defs.get(id));
            let effect = definition.is_some_and(|definition| definition["class"] == "ActorFrame"
                && definition["source"].as_str().is_some_and(|source|
                    source.starts_with("@/")
                        && source.rsplit_once("/BackgroundEffects/").is_some_and(|(_, file)|
                            file.ends_with(".lua") && !file.contains('/'))));
            if effect {
                let actor = input["actor"].as_str().ok_or("invalid native background effect identity")?;
                let children = inputs.iter().filter(|child| child["scene"].as_u64() == Some(scene)
                    && child["parent"].as_str() == Some(actor)).collect::<Vec<_>>();
                if children.len() != 1 {
                    return Err("native background effect does not own one authored entry".into());
                }
                let entry = children[0]["actor"].as_str().ok_or("invalid native background entry identity")?;
                background_effects.insert(actor);
                background_owners.push((entry, true));
            } else if let Some(parent) = parent {
                if !background_owners.iter().any(|&(owner, _)| owner == parent) {
                    background_owners.push((parent, false));
                }
            }
        }
        if background_owners.len() != background_entries {
            // MakeActor returns an unobserved plain Actor after a Lua load
            // failure. When every observed tree is owned by Foreground, retain
            // its independent entry slots without inventing background ranks.
            let only_foreground_roots = !foreground_roots.is_empty()
                && inputs.iter().filter(|input| input["scene"].as_u64() == Some(scene)
                    && input["parent"].as_str().is_none_or(|parent| !instances.contains_key(parent)))
                    .all(|input| input["actor"].as_str().is_some_and(|actor| foreground_roots.contains_key(actor)));
            if !background_owners.is_empty() || !only_foreground_roots {
                return Err("native background containers do not match the prepared entry count".into());
            }
            pending_coverage.push(format!("native scene {scene}: no captured factory identity for {background_entries} background entries; foreground ranks retained"));
        }
        for (index, (owner, instantiated)) in background_owners.into_iter().enumerate() {
            if instantiated {
                background_roots.insert(owner, (index + 1).to_string());
            } else {
                paths.insert(owner, (index + 1).to_string());
            }
        }
    }
    let mut child_counts = BTreeMap::new();
    let mut pointers = BTreeSet::new();
    let mut result = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        let definition = input["definition_id"].as_str().ok_or("missing creation definition")?;
        let actor = input["actor"].as_str().ok_or("missing creation actor")?;
        let pointer = input["pointer_hex"].as_str().ok_or("missing native subscriber pointer")?;
        if pointer.is_empty() || !pointer.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
            return Err("invalid native subscriber pointer".into());
        }
        let pointer = usize::from_str_radix(pointer, 16).map_err(|_| "native subscriber pointer exceeds host width")?;
        if pointer == 0 || !observed.insert(actor)
            || instances.get(actor).copied() != Some(definition)
            || input["scene"].as_u64().is_none_or(|scene| scene == 0)
            || input["ordinal"].as_u64() != Some(index as u64 + 1)
            || input["before_seq"].as_u64().is_none_or(|seq| seq == 0 || seq > event_count as u64 + 1)
            || input["provenance"] != "native-ActorUtil-before-LoadFromNode"
        {
            return Err("invalid native actor creation input".into());
        }
        if let Some(parent) = input["parent"].as_str().filter(|parent| instances.contains_key(parent)) {
            let parent_input = inputs.iter().find(|record| record["actor"].as_str() == Some(parent))
                .ok_or("native factory parent has no creation identity")?;
            if parent_input["scene"] != input["scene"] {
                return Err("native factory parent belongs to another scene".into());
            }
        }
        if input["scene"].as_u64() != Some(scene) { continue; }
        if !pointers.insert(pointer) { return Err("duplicate native subscriber pointer in scene".into()); }
        if background_effects.contains(actor) { continue; }
        if !foreground_roots.is_empty() && !foreground_roots.contains_key(actor)
            && !background_roots.contains_key(actor)
            && input["parent"].as_str().is_none_or(|parent| !paths.contains_key(parent))
        {
            continue;
        }
        let path = if let Some(path) = foreground_roots.get(actor) {
            path.clone()
        } else if let Some(path) = background_roots.get(actor) {
            path.clone()
        } else if let Some(parent) = input["parent"].as_str().filter(|parent| paths.contains_key(parent)) {
            let parent_path: &String = paths.get(parent).ok_or("native factory parent is outside this scene or not yet created")?;
            let child = child_counts.entry(parent).or_insert(0usize);
            *child += 1;
            format!("{parent_path}/{child}")
        } else {
            if input["parent"].as_str().is_some_and(|parent| instances.contains_key(parent)) {
                return Err("native factory parent is outside this scene or not yet created".into());
            }
            if !input["parent"].is_null() && !runtime.iter().any(|record| record["id"] == input["parent"]) {
                return Err("native factory parent has no runtime identity".into());
            }
            if !result.is_empty() {
                return Err("native subscriber ownership needs a single instantiated root per scene".into());
            }
            (background_entries + 1).to_string()
        };
        if paths.insert(actor, path.clone()).is_some() {
            return Err("duplicate native factory instance".into());
        }
        result.push((path, pointer));
    }
    if result.is_empty() { return Err("native scene has no instantiated actor root".into()); }
    Ok((result, pending_coverage))
}

// The overlay records progress in frame order. Pause/unpause can move the
// audio clock backward, so a later sample must not be consumed at an earlier
// overshoot. Observer clock inputs select the phase; scores remain independent.
fn native_progress_observer_frames(samples: &[Value], frames: &[Value]) -> Result<BTreeMap<usize, usize>, String> {
    let mut result = BTreeMap::new();
    let mut cursor = 0;
    for (sample_index, sample) in samples.iter().enumerate().filter(|(_, sample)|
        matches!(sample["phase"].as_str(), Some("progress" | "screen-end" | "gameplay-exit"))) {
        let seconds = number(sample, "seconds")?;
        let ending = sample["phase"] != "progress";
        let matches_clock = |frame: &Value| frame["scene"] == sample["scene"]
            && frame["seconds"].as_f64().is_some_and(|time| (time - seconds).abs() <= 1e-6);
        // Ending observers run after the scene's final update. Both ending
        // samples can describe that same update; an audio-clock overshoot in
        // the preceding frame must not consume them early.
        let relative = if ending { frames[cursor..].iter().rposition(matches_clock) }
            else { frames[cursor..].iter().position(matches_clock) };
        if let Some(relative) = relative {
            let index = cursor + relative;
            result.insert(sample_index, index);
            cursor = index + usize::from(!ending);
        }
    }
    Ok(result)
}

fn native_random_seed(trace: &Value) -> Result<u32, String> {
    if trace.get("random_seed").is_none() && trace["capabilities"]["native_random_seed"] != true {
        // Preserve the previous diagnostic behavior for unseeded captures.
        // Their random effects still require a source-bound seeded capture.
        return Ok(1);
    }
    trace["random_seed"].as_u64().filter(|&seed| seed > 0 && seed <= i32::MAX as u64)
        .map(|seed| seed as u32).ok_or_else(|| "native Lua random seed is missing or invalid".into())
}

fn native_autoplay(trace: &Value) -> Result<bool, String> {
    match trace["input"]["autoplay"].as_u64() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err("native controller must be human or full autoplay; other inputs remain pending".into()),
    }
}

fn native_theme_timing_windows(trace: &Value) -> Result<[[bool; 5]; 2], String> {
    // SL_Init.lua enables every window for a fresh profile. Retain the real
    // initial theme mask independently of the engine's disabled-window bits.
    let mut windows = [[true; 5]; 2];
    for (player, enabled) in windows.iter_mut().enumerate() {
        if let Some(value) = trace.pointer("/input/gameplay_preferences/players")
            .and_then(Value::as_array).and_then(|players| players.get(player))
            .and_then(|player| player.pointer("/theme_modifiers/TimingWindows"))
        {
            *enabled = serde_json::from_value(value.clone())
                .map_err(|error| format!("initial TimingWindows must contain five booleans: {error}"))?;
        }
    }
    Ok(windows)
}

fn native_profile_inputs(trace: &Value) -> Result<[deadsync_profile::Profile; 2], String> {
    // The isolated probe starts with a fresh native profile. GameState's
    // default player options use the inherited Common.DefaultModifiers=1x.
    // Keep nonempty machine defaults pending instead of silently substituting
    // DeadSync's guest profile or reconstructing options from later queries.
    if trace
        .pointer("/input/gameplay_preferences/default_modifiers")
        .and_then(Value::as_str)
        != Some("")
    {
        return Err(
            "nonempty native default modifiers require complete initial option support".into(),
        );
    }
    let mut profiles = std::array::from_fn(|player| deadsync_profile::Profile {
        scroll_speed: deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0),
        // A neutral profile input, recorded before chart Lua executes. Older
        // captures use SL_Init.lua's fresh-profile default of enabled cues.
        column_cues: trace
            .pointer("/input/gameplay_preferences/players")
            .and_then(Value::as_array)
            .and_then(|players| players.get(player))
            .and_then(|player| player.pointer("/theme_modifiers/ColumnCues"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
        ..Default::default()
    });
    for (player, profile) in profiles.iter_mut().enumerate() {
        if let Some(value) = trace.pointer("/input/gameplay_preferences/players")
            .and_then(Value::as_array).and_then(|players| players.get(player))
            .and_then(|player| player.pointer("/theme_modifiers/BackgroundFilter"))
        {
            let text = value.as_str().ok_or("initial BackgroundFilter must be a named percentage")?;
            profile.background_filter = text.parse()
                .map_err(|error| format!("invalid initial BackgroundFilter: {error}"))?;
        }
        let initial = trace.pointer("/input/gameplay_preferences/players")
            .and_then(Value::as_array).and_then(|players| players.get(player));
        if let Some(value) = initial.and_then(|p| p.pointer("/theme_modifiers/HideDanger")) {
            profile.hide_danger = value.as_bool().ok_or("initial HideDanger must be boolean")?;
        }
        if let Some(value) = initial.and_then(|p| p.pointer("/theme_modifiers/HideLifebar")) {
            profile.hide_lifebar = value.as_bool().ok_or("initial HideLifebar must be boolean")?;
        }
        if let Some(value) = initial.and_then(|p| p.pointer("/theme_modifiers/DataVisualizations")) {
            profile.step_statistics = value.as_str().ok_or("initial DataVisualizations must be named")?
                .parse().map_err(|error| format!("invalid initial DataVisualizations: {error}"))?;
        }
        if let Some(value) = initial.and_then(|p| p.pointer("/theme_modifiers/NPSGraphAtTop")) {
            profile.nps_graph_at_top = value.as_bool().ok_or("initial NPSGraphAtTop must be boolean")?;
        }
        if let Some(value) = initial.and_then(|p| p.pointer("/theme_modifiers/LifeMeterType")) {
            profile.lifemeter_type = value.as_str().ok_or("initial LifeMeterType must be named")?
                .parse().map_err(|error| format!("invalid initial LifeMeterType: {error}"))?;
        }
    }
    Ok(profiles)
}

fn compare_stage_sample(
    state: &gameplay::State,
    song: &deadsync_chart::song::SongData,
    sample: &Value,
    overlay_observation: Option<&deadsync_song_lua::SongLuaOverlayStageObservation>,
    seconds: f64,
    scene: u64,
    mismatches: &mut Vec<Value>,
    observations: &mut Vec<Value>,
) -> Result<usize, String> {
    let mut compared=0;
    for p in 0..state.gameplay.num_players() {
    let player = &state.players_runtime.players[p];
    let score = state.stage_itg_score_inputs(p).ok_or("missing active player score inputs")?;
    let selected = if let Some(observation) = overlay_observation {
        observation.player_step_indices[p]
    } else {
        state.song_lua.as_ref().ok_or("missing Lua metadata")?
            .selected_context().map_err(|error| error.to_string())?.player_step_indices[p]
    }.ok_or("GAMESTATE has no selected P1 chart")?;
    let context = state.song_lua.as_ref().ok_or("missing Lua metadata")?.context();
    let chart_index = if context.song_step_chart_indices.is_empty() { Some(selected) }
        else { context.song_step_chart_indices.get(selected).copied().flatten() }
        .ok_or("GAMESTATE selected an unplayable chart")?;
    let metadata = song.charts.get(chart_index).ok_or("GAMESTATE selected an absent chart")?;
    let actual = if let Some(observation) = overlay_observation {
        let observed = &observation.players[p];
        json!({"possible":observed.possible_points, "current_possible":observed.current_possible_points,
            "actual":observed.actual_points, "misses":observed.tap_scores[4],
            "w1":observed.tap_scores[9], "w2":observed.tap_scores[8],
            "failed":observed.failed, "life_latest":observed.life,
            "difficulty":format!("Difficulty_{}", metadata.difficulty),
            "description":metadata.description, "meter":metadata.meter})
    } else {
        json!({"possible":score.possible_grade_points, "current_possible":player.current_possible_points,
        "actual":player.earned_grade_points, "misses":player.judgment_counts[5],
        "w1":player.judgment_counts[0], "w2":player.judgment_counts[1],
        "failed":player.stage_failed || player.fail_time.is_some(), "life_latest":player.life,
        "difficulty":format!("Difficulty_{}", metadata.difficulty),
        "description":metadata.description, "meter":metadata.meter})
    };
    let native = &sample["players"][p];
    let fields = ["possible", "current_possible", "actual", "misses", "w1", "w2",
        "failed", "life_latest", "difficulty", "description", "meter"];
    for key in fields {
        let equal = if key == "life_latest" {
            (number(native, key)? - number(&actual, key)?).abs() <= 1e-6
        } else {
            native[key] == actual[key]
        };
        if !equal {
            mismatches.push(json!({"phase":sample["phase"],"seconds":seconds,
                "player_index":p,"field":key,"native":native[key],"deadsync":actual[key]}));
        }
    }
    observations.push(json!({"phase":sample["phase"],"scene":scene,"seconds":seconds,
        "beat":state.clock.song_position.current_beat,
        "screen_seconds":state.boundary.total_elapsed_in_screen,"player_index":p,"player":actual}));
    compared += fields.len();
    }
    Ok(compared)
}

fn native_gameplay_session(trace: &Value) -> Result<GameplaySession, String> {
    use deadsync_gameplay::GameplayInputPlayStyle;
    // GameState::SetCurrentStyle can change the final observed style during a
    // gameplay reload. Replay must begin with the captured launch input.
    let play_style = match trace["input"]["style"].as_str().or_else(|| trace["style"].as_str()) {
        Some("single") => GameplayInputPlayStyle::Single,
        Some("versus") => GameplayInputPlayStyle::Versus,
        Some("double") => GameplayInputPlayStyle::Double,
        _ => return Err("live replay currently supports single, versus and double; other styles remain pending".into()),
    };
    Ok(GameplaySession { play_style, joined_sides: [true,play_style.is_versus()], ..Default::default() })
}

fn native_starting_chart(
    song: &deadsync_chart::SongData, session: &GameplaySession, difficulty: Option<&str>,
) -> Result<usize, String> {
    let steps_type = if session.play_style.is_double() { "dance-double" } else { "dance-single" };
    song.charts.iter().position(|chart| chart.chart_type == steps_type
        && difficulty.is_none_or(|wanted| chart.difficulty.eq_ignore_ascii_case(wanted)))
        .ok_or_else(|| "requested native starting chart is absent".into())
}

pub(crate) fn replay(
    trace: &Value,
    trace_path: &Path,
    simfile: &Path,
    last_seen_video_driver: Option<&str>,
    scope: Scope,
    progress: impl FnMut(usize, usize),
) -> Result<Value, String> {
    replay_with_command_observer(trace, trace_path, simfile, last_seen_video_driver,
        false, scope, progress)
}

pub(crate) fn replay_with_command_observer(
    trace: &Value,
    trace_path: &Path,
    simfile: &Path,
    last_seen_video_driver: Option<&str>,
    dynamic_commands: bool,
    scope: Scope,
    mut progress: impl FnMut(usize, usize),
) -> Result<Value, String> {
    // Production resolves song media from an absolute root. Use that same
    // identity for resource checks and authored-query filtering when the CLI
    // receives a relative simfile path.
    let absolute_simfile = std::path::absolute(simfile).map_err(|error| error.to_string())?;
    let simfile = absolute_simfile.as_path();
    validate_capture(&trace)?;
    let autoplay = native_autoplay(&trace)?;
    let session = native_gameplay_session(trace)?;
    let frames = trace["update_frames"]
        .as_array()
        .filter(|frames| !frames.is_empty())
        .ok_or("native update frames are empty")?;
    let last_scene = frames
        .last()
        .and_then(|frame| frame["scene"].as_u64())
        .ok_or("missing final scene")?;
    // Stop just after the first reload, or at the first nonnegative input
    // clock for a song that does not reload. Every preceding frame is retained.
    let frame_count = if scope == Scope::FirstQuery {
        let query = trace["events"]
            .as_array()
            .ok_or("missing native events")?
            .iter()
            .find(|event| {
                matches!(
                    event["operation"].as_str(),
                    Some(
                        "PlayerStageStats.GetCurrentPossibleDancePoints"
                            | "PlayerStageStats.GetPossibleDancePoints"
                    )
                )
            })
            .ok_or("native capture has no authored point queries")?;
        let query_scene = query["scene"]
            .as_u64()
            .ok_or("native point query has no scene epoch")?;
        let at = number(query, "seconds")?;
        frames
            .iter()
            .position(|frame| {
                frame["scene"].as_u64() == Some(query_scene)
                    && frame["seconds"]
                        .as_f64()
                        .is_some_and(|seconds| seconds >= at - 1e-6)
            })
            .ok_or("native point query has no matching gameplay frame")?
            + 1
    } else if let Scope::ThroughSecond(at) = scope {
        if !at.is_finite() || at < 0.0 {
            return Err("diagnostic replay cutoff must be finite and nonnegative".into());
        }
        frames
            .iter()
            .position(|frame| frame["seconds"].as_f64().is_some_and(|seconds| seconds >= at))
            .ok_or("native capture does not reach the diagnostic replay cutoff")?
            + 1
    } else if scope == Scope::Entry {
        frames
            .iter()
            .position(|frame| frame["scene"].as_u64().is_some_and(|scene| scene > 1))
            .or_else(|| {
                frames
                    .iter()
                    .position(|frame| frame["seconds"].as_f64().is_some_and(|time| time >= 0.0))
            })
            .map_or(frames.len(), |index| index + 1)
    } else {
        frames.len()
    };
    progress(0, frame_count);
    let samples = trace["stage_samples"]
        .as_array()
        .ok_or("missing native stage observations")?;
    let progress_observer_frames = native_progress_observer_frames(samples, frames)?;
    let difficulty = trace["input"]["requested_difficulty"].as_str();
    let preferences = &trace["input"]["preferences"];
    let sync = match preferences["default_sync_offset"].as_str() {
        Some("SyncOffset_NULL") => deadsync_chart::SyncPref::Null,
        Some("SyncOffset_ITG") => deadsync_chart::SyncPref::Itg,
        _ => return Err("native default sync setting is missing or unknown".into()),
    };
    let config = GameplayConfig {
        default_fail_type: native_fail_type(&trace)?,
        global_offset_seconds: number(preferences, "global_offset_seconds")? as f32,
        visual_delay_seconds: number(preferences, "visual_delay_seconds")? as f32,
        machine_default_sync_pref: sync,
        machine_pack_ini_offsets: true,
        ..Default::default()
    };
    let limit = u32::try_from(trace.pointer("/capture_limits/max_events")
        .and_then(Value::as_u64).unwrap_or(250_000))
        .map_err(|_| "native call trace capacity exceeds u32")?;
    start_call_log(limit as usize)?;
    paths::init();
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(
        simfile,
        config.global_offset_seconds,
    )?);
    let initial = native_starting_chart(&song, &session, difficulty)?;
    let first = Arc::new(song.charts[initial].clone());
    let charts = [first.clone(), first];
    let initial_payload = Arc::new(
        deadsync_simfile::app_runtime::load_gameplay_charts(
            &song,
            &[initial],
            config.global_offset_seconds,
        )?
        .remove(0),
    );
    let payloads = [initial_payload.clone(), initial_payload];
    let profiles = native_profile_inputs(&trace)?;
    let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
    let viewport = GameplayViewport::new(
        number(&trace["display"], "logical_width")? as f32,
        number(&trace["display"], "logical_height")? as f32,
    );
    let mut context = deadsync_shell::gameplay_entry::song_lua_context(
        &song,
        &charts,
        std::array::from_fn(|p| &payloads[p].timing),
        &profiles,
        &scroll,
        1.0,
        viewport,
        (
            number(&trace["display"], "width")? as u32,
            number(&trace["display"], "height")? as u32,
        ),
        // Older captures use the pinned harness's fresh PrefsManager default;
        // its Preferences.ini sets dimensions but never overrides this value.
        preferences.get("display_aspect_ratio").and_then(Value::as_f64)
            .map_or(16.0 / 9.0, |value| value as f32),
        f32::from(autoplay),
        &session,
        &config,
        trace["display"]["video_renderers"].as_str().unwrap_or("null"),
        native_random_seed(trace)?,
        true,
    )?;
    // The recorded renderer is machine input. Keep its exact preference list:
    // charts can select an entirely different actor tree when AFTs are disabled.
    context.video_renderers = trace["display"]["video_renderers"].as_str().unwrap_or("null").to_owned();
    for (player, windows) in context.players.iter_mut().zip(native_theme_timing_windows(trace)?) {
        player.timing_windows = windows;
    }
    // Optional input comes from retained machine configuration or the native
    // input snapshot. Query observations remain expectations only.
    context.last_seen_video_driver = last_seen_video_driver
        .or_else(|| preferences.get("last_seen_video_driver").and_then(Value::as_str))
        .unwrap_or_default().to_string();
    // This is a capture-tool observation contract, not a gameplay input.
    // No native command events, query results or expected values enter the VM.
    context.trace_dynamic_commands = dynamic_commands;
    let (creation_order, factory_coverage) = native_creation_order_with_coverage_for_entries(
        trace, 1, song.background_lua_changes.len())?;
    context.message_actor_order = creation_order;
    // Native ScreenGameplay updates song position before its first Update(0).
    // Defer preparation's placeholder update and run every recorded frame,
    // including zero-delta inputs, through the actual gameplay entry point.
    assert!(context.defer_initial_live_update, "production preparation must wait for the screen clock");
    let prepared = deadsync_song_lua::playback::prepare_song_lua(
        &song, &context, deadsync_assets::song_lua::compile_song_lua,
        deadsync_assets::song_lua::compile_song_lua_layers,
    );
    let observe_overlay = trace["update_frame_phase"].as_str() == Some("gameplay-overlay-probe");
    let init_scene = |prepared: deadsync_song_lua::playback::PreparedGameplaySongLua<
        deadsync_assets::noteskin::SpriteSlot,
    >|
     -> Result<Box<gameplay::State>, String> {
        let mut charts = charts.clone();
        let mut payloads = payloads.clone();
        let mut session = session.clone();
        deadsync_shell::gameplay_entry::apply_song_lua_reload(
            &song,
            &mut charts,
            &mut payloads,
            &prepared,
            &mut session,
            config.global_offset_seconds,
        )?;
        let mut state = Box::new(gameplay::init(
            song.clone(),
            charts,
            payloads,
            viewport,
            session.clone(),
            config,
            5,
            1.0,
            scroll,
            profiles.clone(),
            None,
            None,
            None,
            Arc::from("EVENT"),
            None,
            None,
            None,
            None,
            None,
            [deadsync_gameplay::CourseLifeConfig::Bar; 2],
            false,
            None,
            None,
            [0; 2],
            prepared,
            Default::default(),
        ));
        if state.song_lua.is_none() {
            return Err("preparation did not retain a Lua scene".into());
        }
        // This is a recorded controller input, not an expected score. Native
        // Player::HandleTapRowScore/HandleHoldScore skip release autoplay
        // scoring; run the production autoplay path with the same input.
        state.set_live_autoplay_enabled(autoplay);
        if observe_overlay {
            state.song_lua.as_ref().expect("validated Lua scene")
                .enable_overlay_stage_observer().map_err(|error| error.to_string())?;
        }
        Ok(state)
    };
    let mut state = init_scene(prepared)?;
    let mut draw_checks = interactive::DrawChecks::default();
    let draw_samples = trace["drawable_samples"].as_array().map(Vec::as_slice).unwrap_or_default();
    let input_events = trace["input_events"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mut input_cursor = 0;
    let mut draw_cursor = 0;
    if trace["capabilities"]["native_drawable_vertices"] == true {
        draw_checks.bind(&state, trace, 1)?;
    }

    let startup = state.take_song_lua_output();
    let startup_scenes = startup
        .broadcasts
        .iter()
        .filter(|(_, name, _)| name == "ScreenChanged")
        .count();
    let mut mismatches = Vec::new();
    let mut scene_count = startup_scenes as u64;
    let mut pending_reload = false;
    let mut deferred_reload = false;
    let mut errors = startup.errors;
    let mut runtime_coverage: BTreeSet<String> = startup.coverage_gaps.into_iter().collect();
    runtime_coverage.extend(factory_coverage);
    let mut broadcasts: Vec<_> = startup
        .source_broadcasts
        .iter()
        .map(|event| broadcast_event(event, simfile))
        .collect();
    let mut observations = Vec::new();
    let mut requests = Vec::new();
    let mut first_navigation = None;
    let mut frames_after_navigation = 0;
    let mut checked_samples = 0;
    let mut compared_fields = 0;
    let mut sample_cursor = 0;
    let mut previous_wall = None;
    let mut used_frames = 0;
    let mut starting_scene = true;
    for (index, frame) in frames.iter().take(frame_count).enumerate() {
        if first_navigation.is_some() {
            frames_after_navigation += 1;
        }
        if frame["sequence"].as_u64() != Some(index as u64 + 1) {
            return Err("native frame sequence is incomplete".into());
        }
        if pending_reload {
            let mut scene = state
                .song_lua
                .take()
                .ok_or("reload has no retained Lua scene")?;
            let (creation_order, factory_coverage) = native_creation_order_with_coverage_for_entries(
                trace, scene_count + 1, scene.context().background_layer_count)?;
            runtime_coverage.extend(factory_coverage);
            scene.set_message_actor_order(creation_order);
            let prepared = deadsync_song_lua::playback::reload_song_lua(
                &song,
                scene,
                deadsync_assets::song_lua::reload_song_lua,
            )?;
            state = init_scene(prepared)?;
            let startup = state.take_song_lua_output();
            errors.extend(startup.errors);
            runtime_coverage.extend(startup.coverage_gaps);
            broadcasts.extend(
                startup
                    .source_broadcasts
                    .iter()
                    .map(|event| broadcast_event(event, simfile)),
            );
            scene_count += 1;
            starting_scene = true;
            if trace["capabilities"]["native_drawable_vertices"] == true {
                draw_checks.bind(&state, trace, scene_count)?;
            }

        }
        if frame["scene"].as_u64() != Some(scene_count) {
            return Err(format!(
                "native scene {} differs from independent scene {scene_count} at frame {}; preceding callback errors: {:?}",
                frame["scene"], frame["sequence"], errors.iter().rev().take(5).collect::<Vec<_>>()
            ));
        }
        let seconds = number(frame, "seconds")?;
        let delta = number(frame, "delta")?;
        let wall = number(frame, "wall_seconds")?;
        if delta < 0.0 || previous_wall.is_some_and(|previous| wall < previous) {
            return Err("invalid native input-clock order".into());
        }
        previous_wall = Some(wall);
        if starting_scene {
            // ScreenManager::LoadScreen broadcasts ScreenChanged before the
            // first overlay Update. Its observer clock can precede that frame
            // while the initialized scores already exist. Replay only this
            // clock input here; never run an extra frame or import native stats.
            while let Some(sample) = samples.get(sample_cursor) {
                if sample["phase"] == "overlay-init"
                    || sample["scene"].as_u64().is_some_and(|scene| scene < scene_count)
                {
                    sample_cursor += 1;
                    continue;
                }
                if sample["phase"] != "screen-active" || sample["scene"].as_u64() != Some(scene_count) {
                    break;
                }
                let at = number(sample, "seconds")?;
                if at > seconds + 1e-6 {
                    return Err("native activation clock follows the first gameplay frame".into());
                }
                state.set_current_music_time_ns((at * 1e9).round() as i64);
                compared_fields += compare_stage_sample(&state, &song, sample, None, at,
                    scene_count, &mut mismatches, &mut observations)?;
                checked_samples += 1;
                sample_cursor += 1;
            }
            starting_scene = false;
        }
        let host = (wall * 1e9).round() as u64;
        let snapshot = GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot {
                music_nanos: (seconds * 1e9).round() as i64,
                has_music_mapping: true,
                valid_at_host_nanos: host,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut effects = Vec::new();
        let mut input_error=None;
        gameplay::update_with_input(&mut state, delta as f32, snapshot, || host, &mut effects, |state| {
        while let Some(event) = input_events.get(input_cursor) {
            if event["before_update_sequence"].as_u64() != Some(index as u64+2) { break; }
            if let Err(error)=interactive::dispatch(state, &event["event"]) { input_error=Some(error); break; }
            input_cursor += 1;
        }
        });
        if let Some(error)=input_error { return Err(error); }
        let draw_start = draw_cursor;
        while draw_samples.get(draw_cursor).is_some_and(|sample|
            sample["update_sequence"].as_u64() == Some(index as u64+1)) { draw_cursor += 1; }
        if draw_cursor > draw_start {
            draw_checks.compare(&state, &draw_samples[draw_start..draw_cursor],
                [context.screen_width, context.screen_height], [seconds as f32, number(frame,"beat")? as f32],
                simfile, &mut mismatches)?;
        }
        let overlay_observation = state.song_lua.as_ref()
            .and_then(|scene| scene.take_overlay_stage_observation());
        if observe_overlay && overlay_observation.is_none() {
            runtime_coverage.insert("owned gameplay overlay observation phase is unavailable".into());
        }
        used_frames += 1;
        progress(used_frames, frame_count);
        pending_reload = effects.iter().any(|effect| {
            matches!(
                effect,
                deadsync_theme_simply_love::SimplyLoveEffect::Runtime(
                    deadsync_theme_simply_love::SimplyLoveRuntimeRequest::ReloadSongLua
                )
            )
        });
        if deferred_reload && !pending_reload { return Err("callback reload did not emit the next production frame effect".into()); }
        deferred_reload=false;
        if first_navigation.is_none()
            && effects.iter().any(|effect| {
                matches!(
                    effect,
                    deadsync_theme_simply_love::SimplyLoveEffect::Navigate(_)
                        | deadsync_theme_simply_love::SimplyLoveEffect::NavigateNoFade(_)
                )
            })
        {
            first_navigation = Some(seconds);
        }
        let output = state.take_song_lua_output();
        broadcasts.extend(
            output
                .source_broadcasts
                .iter()
                .map(|event| broadcast_event(event, simfile)),
        );
        errors.extend(output.errors);
        runtime_coverage.extend(output.coverage_gaps);
        // Production coalesces screen loads, while the source trace records
        // every authored invocation. Validate the queued effect independently.
        if let Some(deadsync_song_lua::SongLuaLiveRequest::NewScreen(name)) = &output.request {
            if name != "ScreenGameplay" {
                return Err(format!("unsupported reload target {name}"));
            }
            if !pending_reload {
                deferred_reload=true;
            }
        }
        for event in output.source_requests {
            let request = event.request;
            let (operation, name) = match request {
                deadsync_song_lua::SongLuaLiveRequest::Transition(name) => {
                    ("StartTransitioningScreen", name)
                }
                deadsync_song_lua::SongLuaLiveRequest::NewScreen(name) => {
                    ("SetNewScreen", name)
                }
            };
            requests.push(json!({"seconds":event.music_seconds, "operation":operation, "args":[name]}));
        }
        // Compare independently simulated state only at the recorded observer
        // phase. Native stats are never fed back into gameplay or the Lua VM.
        while sample_cursor < samples.len() {
            let sample = &samples[sample_cursor];
            let at = number(sample, "seconds")?;
            if !matches!(
                sample["phase"].as_str(),
                Some("screen-active" | "progress" | "screen-end" | "gameplay-exit")
            ) || sample["scene"]
                .as_u64()
                .is_some_and(|scene| scene < scene_count)
            {
                sample_cursor += 1;
                continue;
            }
            if sample["scene"].as_u64() != Some(scene_count) {
                break;
            }
            if progress_observer_frames.get(&sample_cursor).is_some_and(|&frame_index| frame_index > index) {
                break;
            }
            if at > seconds + 1e-6 {
                break;
            }
            sample_cursor += 1;
            if (at - seconds).abs() > 1e-6 {
                mismatches
                    .push(json!({"field":"stage_sample_clock", "native":at,"deadsync":seconds}));
                continue;
            }
            checked_samples += 1;
            compared_fields += compare_stage_sample(&state, &song, sample,
                (sample["phase"] == "progress").then_some(overlay_observation.as_ref()).flatten(), seconds,
                scene_count, &mut mismatches, &mut observations)?;
        }
    }
    if scope != Scope::Full {
        let calls = compare_calls(
            &trace,
            simfile,
            scene_count,
            number(&frames[used_frames - 1], "seconds")?,
            scope,
            &mut mismatches,
        )?;
        let mut coverage = coverage(&trace, scope, &calls, used_frames, frames.len());
        coverage["pending"].as_array_mut().unwrap()
            .extend(runtime_coverage.iter().map(|gap| json!(gap)));
        let mut report = json!({"diagnostic_only":true,"whole_song_parity_proven":false,"archive_accepted":false,
            "scope":match scope { Scope::Entry => "entry", Scope::FirstQuery => "first-point-query",
                Scope::ThroughSecond(_) => "through-second", Scope::Full => "full" },
            "through_second":match scope { Scope::ThroughSecond(at) => Some(at), _ => None },
            "native_trace":trace_path,"source_simfile":simfile,
            "native_frames":frames.len(),"replayed_frames":used_frames,"replayed_scenes":scene_count,
            "checked_stage_samples":checked_samples,"compared_stage_fields":compared_fields,
            "mismatches":mismatches,"errors":errors,"lua_requests":requests,
            "stage_observations":observations,
            "command_queries":null,
            "pending_coverage":coverage["pending"],"coverage":coverage});
        report["command_queries"] = calls;
        return Ok(report);
    }
    let expected_samples = samples
        .iter()
        .filter(|sample| {
            matches!(
                sample["phase"].as_str(),
                Some("screen-active" | "progress" | "screen-end" | "gameplay-exit")
            )
        })
        .count();
    if expected_samples != checked_samples {
        mismatches.push(json!({"field":"stage_sample_coverage", "native":expected_samples,"deadsync":checked_samples}));
    }
    if scene_count != last_scene {
        mismatches
            .push(json!({"field":"scene_coverage", "native":last_scene,"deadsync":scene_count}));
    }
    let native_requests: Vec<_> = trace["events"].as_array().ok_or("missing native events")?.iter()
        .filter_map(|event| {
            let operation = event["operation"].as_str()?;
            if !(operation.ends_with(".StartTransitioningScreen") || operation == "ScreenManager.SetNewScreen") {
                return None;
            }
            Some(json!({"seconds":event["seconds"], "operation":operation.rsplit('.').next(), "args":event["args"]}))
        }).collect();
    let actual_requests: Vec<_> = requests.iter().collect();
    if native_requests.len() != actual_requests.len() {
        mismatches.push(json!({"field":"source_request_count", "native":native_requests.len(), "deadsync":actual_requests.len()}));
    }
    for (native, actual) in native_requests.iter().zip(actual_requests) {
        if native["operation"] != actual["operation"]
            || native["args"] != actual["args"]
            || (number(native, "seconds")? - number(actual, "seconds")?).abs() > 1e-6
        {
            mismatches.push(json!({"field":"source_request", "native":native,"deadsync":actual}));
        }
    }
    if native_requests.iter().any(|request| {
        request["operation"] == "StartTransitioningScreen"
            && request["args"] == json!(["SM_DoNextScreen"])
    }) {
        let end = samples
            .iter()
            .find(|sample| {
                sample["phase"] == "screen-end" && sample["scene"].as_u64() == Some(last_scene)
            })
            .ok_or("native transition has no final screen-end observation")?;
        let native = number(end, "seconds")?;
        if first_navigation.is_none_or(|actual: f64| (actual - native).abs() > 1e-6) {
            mismatches.push(json!({"field":"screen_end_navigation_clock","native":native,"deadsync":first_navigation}));
        }
    }
    let native_broadcasts: Vec<_> = trace["events"].as_array().ok_or("missing native events")?.iter()
        .filter(|event| event["operation"] == "MessageManager.Broadcast")
        .map(|event| json!({"seconds":event["seconds"],"beat":event["beat"],"name":event["args"][0],"args":event["args"]})).collect();
    if native_broadcasts.len() != broadcasts.len() {
        mismatches.push(json!({"field":"source_broadcast_count","native":native_broadcasts.len(),"deadsync":broadcasts.len()}));
    }
    for (native, actual) in native_broadcasts.iter().zip(&broadcasts) {
        if !actual["params_error"].is_null() {
            mismatches
                .push(json!({"field":"source_broadcast_params_unsupported", "deadsync":actual}));
        }
        if native["args"] != actual["args"]
            || (number(native, "seconds")? - number(actual, "seconds")?).abs() > 1e-6
            || (number(native, "beat")? - number(actual, "beat")?).abs() > 1e-6
        {
            mismatches.push(json!({"field":"source_broadcast","native":native,"deadsync":actual}));
        }
    }
    let calls = compare_calls(
        &trace,
        simfile,
        scene_count,
        number(&frames[used_frames - 1], "seconds")?,
        scope,
        &mut mismatches,
    )?;
    let mut coverage = coverage(&trace, scope, &calls, used_frames, frames.len());
    coverage["pending"].as_array_mut().unwrap()
        .extend(runtime_coverage.iter().map(|gap| json!(gap)));
    let mut report = json!({"diagnostic_only":false,"whole_song_parity_proven":false,"archive_accepted":false,
        "source_simfile":simfile,"native_trace":trace_path,"native_frames":frames.len(),"replayed_frames":used_frames,
        "startup_scenes":startup_scenes,"replayed_scenes":scene_count,"selected_description":state.source.charts[0].description,
        "checked_stage_samples":checked_samples,"compared_stage_fields":compared_fields,"mismatches":mismatches,
        "errors":errors,"lua_requests":requests,"native_transition_requests":native_requests,
        "lua_broadcasts":broadcasts,"native_broadcasts":native_broadcasts,
        "first_domain_navigation_seconds":first_navigation, "continued_after_domain_navigation": frames_after_navigation > 0,
        "frames_after_domain_navigation":frames_after_navigation,
        "stage_observations":observations,
        "broadcast_params_compared":true,
        "command_queries":null,
        "pending_coverage":coverage["pending"],"coverage":coverage});
    report["command_queries"] = calls;
    report["interactive"] = draw_checks.report(input_cursor);
    report["capture_completed_gameplay"] = json!(trace["completion"]["reason"] == "gameplay-exit");
    if input_cursor != input_events.len() || draw_cursor != draw_samples.len() {
        report["mismatches"].as_array_mut().unwrap().push(json!({"field":"interactive_observation_coverage",
            "inputs_replayed":input_cursor,"native_inputs":input_events.len(),
            "draws_compared":draw_cursor,"native_draws":draw_samples.len()}));
    }
    report["interactive_checks_passed"] = json!(scope == Scope::Full
        && input_cursor > 0 && input_cursor == input_events.len() && draw_cursor > 0 && draw_cursor == draw_samples.len()
        && report["mismatches"].as_array().is_some_and(Vec::is_empty)
        && report["errors"].as_array().is_some_and(Vec::is_empty));
    let accepted = archive_passes(scope, &report);
    report["archive_accepted"] = json!(accepted);
    report["diagnostic_only"] = json!(!accepted);
    report["whole_song_parity_proven"] = json!(accepted
        && coverage["pending"].as_array().is_some_and(Vec::is_empty));
    report["acceptance_basis"] = json!("full replay of implemented comparisons");

    eprintln!(
        "replayed {used_frames} actual frames; {checked_samples} stage samples; {} mismatches; {} callback errors",
        report["mismatches"].as_array().map_or(0, Vec::len),
        report["errors"].as_array().map_or(0, Vec::len)
    );
    Ok(report)
}

pub(super) fn archive_passes(scope: Scope, report: &Value) -> bool {
    scope == Scope::Full
        && report["capture_completed_gameplay"] != false
        && report["native_frames"].as_u64().is_some_and(|frames|
            frames > 0 && report["replayed_frames"].as_u64() == Some(frames))
        && report["mismatches"].as_array().is_some_and(Vec::is_empty)
        && report["errors"].as_array().is_some_and(Vec::is_empty)
}
