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
        CALL_EVENTS.with_borrow_mut(|capture| {
            if capture.events.len() >= capture.limit {
                capture.dropped += 1;
                return;
            }
            capture.events.push(
                serde_json::from_str(&record.args().to_string())
                    .unwrap_or_else(|error| json!({"kind":"trace-error","error":error.to_string()})),
            );
        });
    }
    fn flush(&self) {}
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
    let (actual, dropped) = CALL_EVENTS.with_borrow(|capture| {
        for event in capture.events.iter().filter(|event| event["kind"] == "trace-error") {
            mismatches.push(json!({"field":"call_trace_error","deadsync":event}));
        }
        let mut actual_handles = BTreeMap::new();
        let actual: Vec<Value> = capture.events
            .iter()
            .filter_map(|event| {
                let source = event["source"].as_str()?.replace('\\', "/");
                let source = source
                    .split_once(&song_dir)
                    .map(|(_, path)| format!("song:/{path}"))
                    .or_else(|| source.starts_with("song:/").then(|| source.clone()))?;
                let mut event = event.clone();
                event["source"] = json!(source);
                normalize_paths(&mut event, &song_dir);
                let scene = event["scene"].as_u64().unwrap_or(0);
                normalize_query_handles(&mut event["args"], &mut actual_handles, scene);
                if matches!(event["kind"].as_str(), Some("query" | "modifier")) {
                    normalize_query_handles(&mut event["results"], &mut actual_handles, scene);
                }
                Some(event)
            })
            .collect();
        (actual, capture.dropped)
    });
    if dropped != 0 {
        mismatches.push(json!({"field":"dropped_call_trace","deadsync":dropped}));
    }
    if trace["capabilities"]["native_event_scenes"] != true {
        return Ok(
            json!({"compared":false,"reason":"native command/query events have no scene epochs","actual":actual}),
        );
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
        .filter(|event| {
            event["operation"]
                .as_str()
                .is_some_and(|operation| CALL_OPERATIONS.contains(&operation))
        })
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
    Ok(
        json!({"compared":true,"native_count":native.len(),"actual_count":actual.len(),"actual":actual,"native":native}),
    )
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

fn normalize_paths(value: &mut Value, song_dir: &str) {
    match value {
        Value::String(value) => {
            let normalized = value.replace('\\', "/");
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
    handles: &mut BTreeMap<(u64, String), usize>,
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
            // Canonicalize allocation IDs by first observation while retaining
            // alias relationships, actor names, and actual named ancestry.
            if values.get("name").is_some_and(Value::is_string)
                && values.get("path").is_some_and(Value::is_string)
                && let Some(id) = values.get("actor").and_then(Value::as_str)
            {
                let next = handles.len() + 1;
                let id = *handles.entry((scene, id.to_string())).or_insert(next);
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
        ] {
            let mut invalid = trace.clone();
            *invalid.pointer_mut(path).expect("input field") = value;
            assert!(super::native_creation_order_for_entries(&invalid,1,1).is_err(), "accepted invalid {path}");
        }
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
    let players = preferences["players"]
        .as_array()
        .filter(|players| players.len() == 1)
        .ok_or("native gameplay inputs must contain exactly one player")?;
    let player = &players[0];
    if player["player"] != "PlayerNumber_P1"
        || player["disabled_timing_windows"] != json!([])
        || player["min_tns_to_hide_notes"] != "TapNoteScore_W3"
    {
        return Err("native player timing modifiers differ; this context remains pending".into());
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
        || trace["completion"]["reason"] != "gameplay-exit"
        || trace["completion"]["stage_committed"] != true
        || trace["update_frame_phase"] != "gameplay-overlay-probe"
        || trace["input"]["injected_inputs"] != false
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
    if trace["capabilities"]["native_actor_creation"] != true { return Ok(Vec::new()); }
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
        return Ok(Vec::new());
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
    if foreground_roots.len() > 1 {
        return Err("multiple native foreground entries need entry-specific ownership".into());
    }
    let foreground_root = foreground_roots.first().copied();
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
                    source.ends_with("/BackgroundEffects/UpperLeft.lua")
                        || source.ends_with("/BackgroundEffects/Centered.lua")));
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
            return Err("native background containers do not match the prepared entry count".into());
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
        if foreground_root.is_some_and(|root| actor != root)
            && !background_roots.contains_key(actor)
            && input["parent"].as_str().is_none_or(|parent| !paths.contains_key(parent))
        {
            continue;
        }
        let path = if let Some(path) = background_roots.get(actor) {
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
            if !result.is_empty() && foreground_root != Some(actor) {
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
    Ok(result)
}

// The overlay records progress in frame order. Pause/unpause can move the
// audio clock backward, so a later sample must not be consumed at an earlier
// overshoot. Observer clock inputs select the phase; scores remain independent.
fn native_progress_observer_frames(samples: &[Value], frames: &[Value]) -> Result<BTreeMap<usize, usize>, String> {
    let mut result = BTreeMap::new();
    let mut cursor = 0;
    for (sample_index, sample) in samples.iter().enumerate().filter(|(_, sample)| sample["phase"] == "progress") {
        let seconds = number(sample, "seconds")?;
        if let Some(relative) = frames[cursor..].iter().position(|frame|
            frame["scene"] == sample["scene"]
                && frame["seconds"].as_f64().is_some_and(|time| (time - seconds).abs() <= 1e-6))
        {
            let index = cursor + relative;
            result.insert(sample_index, index);
            cursor = index + 1;
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

fn native_profile_inputs(trace: &Value) -> Result<[deadsync_profile::Profile; 2], String> {
    // The isolated probe starts with a fresh native profile. GameState's
    // default player options use the inherited Common.DefaultModifiers=1x.
    // Keep nonempty machine defaults pending instead of silently substituting
    // DeadSync's guest profile or reconstructing options from later queries.
    if trace.pointer("/input/gameplay_preferences/default_modifiers").and_then(Value::as_str) != Some("") {
        return Err("nonempty native default modifiers require complete initial option support".into());
    }
    Ok(std::array::from_fn(|_| deadsync_profile::Profile {
        scroll_speed: deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0),
        ..Default::default()
    }))
}

fn compare_stage_sample(
    state: &gameplay::State,
    song: &deadsync_chart::song::SongData,
    sample: &Value,
    seconds: f64,
    scene: u64,
    mismatches: &mut Vec<Value>,
    observations: &mut Vec<Value>,
) -> Result<usize, String> {
    let player = &state.players_runtime.players[0];
    let score = state.stage_itg_score_inputs(0).ok_or("missing active player score inputs")?;
    let selected = state.song_lua.as_ref().ok_or("missing Lua metadata")?
        .selected_context().map_err(|error| error.to_string())?.player_step_indices[0]
        .ok_or("GAMESTATE has no selected P1 chart")?;
    let metadata = song.charts.get(selected).ok_or("GAMESTATE selected an absent chart")?;
    let actual = json!({"possible":score.possible_grade_points, "current_possible":player.current_possible_points,
        "actual":player.earned_grade_points, "misses":player.judgment_counts[5],
        "w1":player.judgment_counts[0], "w2":player.judgment_counts[1],
        "failed":player.stage_failed || player.fail_time.is_some(), "life_latest":player.life,
        "difficulty":format!("Difficulty_{}", metadata.difficulty),
        "description":metadata.description, "meter":metadata.meter});
    let native = &sample["players"][0];
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
                "field":key,"native":native[key],"deadsync":actual[key]}));
        }
    }
    observations.push(json!({"phase":sample["phase"],"scene":scene,"seconds":seconds,
        "beat":state.clock.song_position.current_beat,
        "screen_seconds":state.boundary.total_elapsed_in_screen,"player":actual}));
    Ok(fields.len())
}

pub(crate) fn replay(
    trace: &Value,
    trace_path: &Path,
    simfile: &Path,
    scope: Scope,
    mut progress: impl FnMut(usize, usize),
) -> Result<Value, String> {
    validate_capture(&trace)?;
    let autoplay = native_autoplay(&trace)?;
    if trace["style"] != "single" {
        return Err(
            "live replay currently supports dance-single; other styles remain pending"
                .into(),
        );
    }
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
    let initial = song
        .charts
        .iter()
        .position(|chart| {
            chart.chart_type == "dance-single"
                && difficulty.is_none_or(|wanted| chart.difficulty.eq_ignore_ascii_case(wanted))
        })
        .ok_or("requested native starting chart is absent")?;
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
    let session = GameplaySession::default();
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
        "null",
        native_random_seed(trace)?,
        true,
    )?;
    context.message_actor_order = native_creation_order_for_entries(trace, 1, song.background_lua_changes.len())?;
    let prepared = deadsync_song_lua::playback::prepare_song_lua(
        &song, &context, deadsync_assets::song_lua::compile_song_lua,
        deadsync_assets::song_lua::compile_song_lua_layers,
    );
    let init_scene = |prepared: deadsync_song_lua::playback::PreparedGameplaySongLua<
        deadsync_assets::noteskin::SpriteSlot,
    >|
     -> Result<Box<gameplay::State>, String> {
        let mut charts = charts.clone();
        let mut payloads = payloads.clone();
        deadsync_shell::gameplay_entry::apply_song_lua_reload(
            &song,
            &mut charts,
            &mut payloads,
            &prepared,
            &session,
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
        Ok(state)
    };
    let mut state = init_scene(prepared)?;
    let startup = state.take_song_lua_output();
    let startup_scenes = startup
        .broadcasts
        .iter()
        .filter(|(_, name, _)| name == "ScreenChanged")
        .count();
    let mut mismatches = Vec::new();
    let mut scene_count = startup_scenes as u64;
    let mut pending_reload = false;
    let mut errors = startup.errors;
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
            scene.set_message_actor_order(native_creation_order_for_entries(trace, scene_count + 1,
                scene.context().background_layer_count)?);
            let prepared = deadsync_song_lua::playback::reload_song_lua(
                &song,
                scene,
                deadsync_assets::song_lua::reload_song_lua,
            )?;
            state = init_scene(prepared)?;
            let startup = state.take_song_lua_output();
            errors.extend(startup.errors);
            broadcasts.extend(
                startup
                    .source_broadcasts
                    .iter()
                    .map(|event| broadcast_event(event, simfile)),
            );
            scene_count += 1;
            starting_scene = true;
        }
        if frame["scene"].as_u64() != Some(scene_count) {
            return Err(format!(
                "native scene {} differs from independent scene {scene_count} at frame {}",
                frame["scene"], frame["sequence"]
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
                compared_fields += compare_stage_sample(&state, &song, sample, at,
                    scene_count, &mut mismatches, &mut observations)?;
                checked_samples += 1;
                sample_cursor += 1;
            }
            starting_scene = false;
        }
        // Preparation already performs the real startup Update(0). Set the
        // input clock without running those callbacks a second time.
        if delta == 0.0 {
            state.set_current_music_time_ns((seconds * 1e9).round() as i64);
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
        if delta > 0.0 {
            gameplay::update(&mut state, delta as f32, snapshot, || host, &mut effects);
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
        if let Some(request) = output.request {
            let (operation, name) = match request {
                deadsync_song_lua::SongLuaLiveRequest::Transition(name) => {
                    ("StartTransitioningScreen", name)
                }
                deadsync_song_lua::SongLuaLiveRequest::NewScreen(name) => {
                    if name != "ScreenGameplay" {
                        return Err(format!("unsupported reload target {name}"));
                    }
                    if !pending_reload {
                        return Err(
                            "gameplay reload did not emit the production runtime effect".into()
                        );
                    }
                    ("SetNewScreen", name)
                }
            };
            requests.push(json!({"seconds":seconds, "operation":operation, "args":[name]}));
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
            compared_fields += compare_stage_sample(&state, &song, sample, seconds,
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
        let coverage = coverage(&trace, scope, &calls, used_frames, frames.len());
        return Ok(
            json!({"diagnostic_only":true,"whole_song_parity_proven":false,"archive_accepted":false,
            "scope":match scope { Scope::Entry => "entry", Scope::FirstQuery => "first-point-query",
                Scope::ThroughSecond(_) => "through-second", Scope::Full => "full" },
            "through_second":match scope { Scope::ThroughSecond(at) => Some(at), _ => None },
            "native_trace":trace_path,"source_simfile":simfile,
            "native_frames":frames.len(),"replayed_frames":used_frames,"replayed_scenes":scene_count,
            "checked_stage_samples":checked_samples,"compared_stage_fields":compared_fields,
            "mismatches":mismatches,"errors":errors,"lua_requests":requests,
            "stage_observations":observations,
            "command_queries":calls,
            "pending_coverage":coverage["pending"],"coverage":coverage}),
        );
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
    let coverage = coverage(&trace, scope, &calls, used_frames, frames.len());
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
        "command_queries":calls,
        "pending_coverage":coverage["pending"],"coverage":coverage});
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
        && report["native_frames"].as_u64().is_some_and(|frames|
            frames > 0 && report["replayed_frames"].as_u64() == Some(frames))
        && report["mismatches"].as_array().is_some_and(Vec::is_empty)
        && report["errors"].as_array().is_some_and(Vec::is_empty)
}
