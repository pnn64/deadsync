use deadsync_assets::song_lua::{
    CompiledSongLua, SongLuaCompileContext, SongLuaDifficulty, SongLuaOverlayCommandBlock,
    SongLuaOverlayKind, SongLuaPlayerContext, SongLuaSpeedMod, compile_song_lua_layers,
};
use deadsync_simfile::song::{ParseSongOptions, parse_song_meta_file};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const TRACE_ENV: &str = "ITGMANIA_SONG_LUA_TRACE";
const SIMFILE_ENV: &str = "ITGMANIA_SONG_LUA_SIMFILE";
const DEFAULT_TRACE: &str =
    "tests/fixtures/itgmania-song-lua/Delightful Day/Delightful Day.ssc.semantic.json";
const EPSILON: f32 = 0.002;

#[derive(Deserialize)]
struct NativeTrace {
    oracle: String,
    title: String,
    simfile: String,
    source_simfile: Option<PathBuf>,
    roots: Vec<String>,
    actor_definitions: Vec<NativeDefinition>,
    runtime_actors: Vec<NativeActor>,
    events: Vec<NativeEvent>,
    end_position: NativePosition,
    display: NativeDisplay,
}

#[derive(Deserialize)]
struct NativeDefinition {
    id: String,
    class: String,
    name: Option<String>,
    #[serde(default)]
    children: Vec<NativeChild>,
}

#[derive(Deserialize)]
struct NativeChild {
    layer_index: usize,
    definition_id: String,
}

#[derive(Deserialize)]
struct NativeActor {
    id: String,
    path: String,
}

#[derive(Deserialize)]
struct NativeEvent {
    kind: String,
    beat: Option<f32>,
    actor: Option<String>,
    operation: String,
    #[serde(default)]
    args: Vec<Value>,
    command: Option<String>,
}

#[derive(Deserialize)]
struct NativePosition {
    seconds: f32,
}

#[derive(Deserialize)]
struct NativeDisplay {
    width: f32,
    height: f32,
    logical_width: f32,
    logical_height: f32,
}

#[derive(Default)]
struct ExpectedBlock {
    duration: f32,
    easing: Option<&'static str>,
    alpha: Option<f32>,
    zoom: Option<f32>,
    rot_z: Option<f32>,
}

struct ExpectedCommand {
    message: String,
    target: NativeTarget,
    blocks: Vec<ExpectedBlock>,
}

enum NativeTarget {
    Layer(usize),
    Player(usize),
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("deadsync should have a workspace parent")
        .to_owned()
}

fn read_trace() -> NativeTrace {
    let path = std::env::var_os(TRACE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_TRACE));
    serde_json::from_slice(
        &fs::read(&path).unwrap_or_else(|error| {
            panic!("failed to read native trace {}: {error}", path.display())
        }),
    )
    .unwrap_or_else(|error| panic!("invalid native trace {}: {error}", path.display()))
}

fn locate_simfile(trace: &NativeTrace) -> PathBuf {
    if let Some(path) = std::env::var_os(SIMFILE_ENV).map(PathBuf::from) {
        return path;
    }
    if let Some(path) = trace.source_simfile.as_ref().filter(|path| path.is_file()) {
        return path.clone();
    }
    let filename = trace
        .simfile
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .expect("native trace has no simfile filename");
    let corpus = workspace_root().join("lua-songs");
    let mut matches = Vec::new();
    let mut pending = vec![corpus];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("failed to scan {}: {error}", directory.display()))
        {
            let path = entry.expect("failed to read corpus entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(filename))
            {
                matches.push(path);
            }
        }
    }
    assert_eq!(
        matches.len(),
        1,
        "could not uniquely locate `{filename}` for trace title `{}`; set {SIMFILE_ENV}",
        trace.title
    );
    matches.pop().expect("one simfile match was required")
}

fn parse_song(path: &Path) -> deadsync_chart::SongData {
    parse_song_meta_file(
        path,
        &ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.0,
        |_| 0.0,
    )
    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn compile_trace_song(trace: &NativeTrace) -> CompiledSongLua {
    let simfile = locate_simfile(trace);
    let simfile = fs::canonicalize(&simfile)
        .unwrap_or_else(|error| panic!("failed to resolve {}: {error}", simfile.display()));
    let song = parse_song(&simfile);
    let mut paths = song
        .background_lua_changes
        .iter()
        .map(|change| change.path.as_path())
        .collect::<Vec<_>>();
    paths.extend(
        song.foreground_lua_changes
            .iter()
            .map(|change| change.path.as_path()),
    );
    assert!(
        !paths.is_empty(),
        "{} has no song Lua layers",
        simfile.display()
    );
    let primary_index = song
        .foreground_lua_changes
        .iter()
        .position(|change| change.start_beat <= 0.0)
        .map(|index| song.background_lua_changes.len() + index)
        .unwrap_or(0);
    let mut context = SongLuaCompileContext::new(
        simfile.parent().unwrap_or_else(|| Path::new(".")),
        song.title.clone(),
    );
    context.song_display_bpms = [song.min_bpm as f32, song.max_bpm as f32];
    context.music_length_seconds = trace.end_position.seconds;
    context.screen_width = trace.display.logical_width;
    context.screen_height = trace.display.logical_height;
    context.display_width = trace.display.width;
    context.display_height = trace.display.height;
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(1.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(1.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let mut compiled =
        compile_song_lua_layers(&paths, primary_index, &context).unwrap_or_else(|error| {
            panic!("DeadSync could not compile {}: {error}", simfile.display())
        });
    assert_eq!(
        compiled.len(),
        1,
        "semantic trace comparison currently requires one active Lua layer"
    );
    compiled.pop().expect("one compiled layer was required")
}

fn kind_name(kind: &SongLuaOverlayKind) -> &'static str {
    match kind {
        SongLuaOverlayKind::Actor => "Actor",
        SongLuaOverlayKind::ActorFrame => "ActorFrame",
        SongLuaOverlayKind::UpdateTracks { .. } => "UpdateTracks",
        SongLuaOverlayKind::ActorFrameTexture { .. } => "ActorFrameTexture",
        SongLuaOverlayKind::ActorProxy { .. } => "ActorProxy",
        SongLuaOverlayKind::AftSprite { .. } => "AftSprite",
        SongLuaOverlayKind::Sprite { .. } => "Sprite",
        SongLuaOverlayKind::Sound { .. } => "Sound",
        SongLuaOverlayKind::BitmapText { .. } => "BitmapText",
        SongLuaOverlayKind::ActorMultiVertex { .. } => "ActorMultiVertex",
        SongLuaOverlayKind::Model { .. } => "Model",
        SongLuaOverlayKind::NoteskinActor { .. } => "NoteskinActor",
        SongLuaOverlayKind::SongMeterDisplay { .. } => "SongMeterDisplay",
        SongLuaOverlayKind::GraphDisplay { .. } => "GraphDisplay",
        SongLuaOverlayKind::Quad => "Quad",
    }
}

fn compare_layers(trace: &NativeTrace, compiled: &CompiledSongLua, gaps: &mut Vec<String>) {
    let definitions = trace
        .actor_definitions
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect::<HashMap<_, _>>();
    let Some(root) = trace
        .roots
        .first()
        .and_then(|root| definitions.get(root.as_str()).copied())
    else {
        gaps.push("native trace has no actor-definition root".into());
        return;
    };
    let mut children = root.children.iter().collect::<Vec<_>>();
    children.sort_by_key(|child| child.layer_index);
    let native = children
        .iter()
        .filter_map(|child| definitions.get(child.definition_id.as_str()).copied())
        .map(|definition| (definition.class.as_str(), definition.name.as_deref()))
        .collect::<Vec<_>>();
    let deadsync = compiled
        .overlays
        .iter()
        .map(|overlay| (kind_name(&overlay.kind), overlay.name.as_deref()))
        .collect::<Vec<_>>();
    if native != deadsync {
        gaps.push(format!(
            "layer order differs\n  ITGmania: {native:?}\n  DeadSync: {deadsync:?}"
        ));
    }
}

fn starred_mods(value: &str) -> String {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| part.starts_with('*'))
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(", ")
}

fn compare_timeline(trace: &NativeTrace, compiled: &CompiledSongLua, gaps: &mut Vec<String>) {
    for event in &trace.events {
        let Some(beat) = event.beat else { continue };
        if event.kind == "message" {
            let Some(message) = event.args.first().and_then(Value::as_str) else {
                continue;
            };
            if !compiled
                .messages
                .iter()
                .any(|actual| actual.message == message && (actual.beat - beat).abs() <= 0.1)
            {
                gaps.push(format!(
                    "missing message `{message}` near beat {beat:.3} (operation {})",
                    event.operation
                ));
            }
        } else if event.kind == "modifier" {
            let Some(raw) = event.args.get(1).and_then(Value::as_str) else {
                continue;
            };
            let wanted = starred_mods(raw);
            if wanted.is_empty() {
                continue;
            }
            if !compiled.beat_mods.iter().any(|actual| {
                (actual.start - beat).abs() <= 0.1 && starred_mods(&actual.mods) == wanted
            }) {
                gaps.push(format!(
                    "missing modifier `{wanted}` near beat {beat:.3} for {}",
                    event.actor.as_deref().unwrap_or("unknown player")
                ));
            }
        }
    }
}

fn value_f32(value: Option<&Value>) -> Option<f32> {
    value.and_then(Value::as_f64).map(|value| value as f32)
}

fn expected_blocks(events: &[&NativeEvent]) -> Vec<ExpectedBlock> {
    let mut blocks = Vec::new();
    for event in events {
        let method = event
            .operation
            .rsplit('.')
            .next()
            .unwrap_or(&event.operation);
        let tween = match method {
            "linear" => Some("linear"),
            "accelerate" => Some("inQuad"),
            "decelerate" => Some("outQuad"),
            "smooth" => Some("inOutQuad"),
            _ => None,
        };
        if let Some(easing) = tween {
            blocks.push(ExpectedBlock {
                duration: value_f32(event.args.first()).unwrap_or_default(),
                easing: Some(easing),
                ..ExpectedBlock::default()
            });
            continue;
        }
        if blocks.is_empty() {
            blocks.push(ExpectedBlock::default());
        }
        let block = blocks.last_mut().expect("an expected block was created");
        match method {
            "diffusealpha" => block.alpha = value_f32(event.args.first()),
            "zoom" => block.zoom = value_f32(event.args.first()),
            "addrotationz" | "rotationz" => block.rot_z = value_f32(event.args.first()),
            _ => {}
        }
    }
    blocks
}

fn trace_commands(trace: &NativeTrace) -> Vec<ExpectedCommand> {
    let paths = trace
        .runtime_actors
        .iter()
        .map(|actor| (actor.id.as_str(), actor.path.as_str()))
        .collect::<HashMap<_, _>>();
    let layers = trace
        .actor_definitions
        .iter()
        .filter_map(|definition| {
            definition
                .children
                .is_empty()
                .then_some((definition.id.as_str(), definition))
        })
        .filter_map(|(id, _definition)| {
            trace.actor_definitions.iter().find_map(|parent| {
                parent
                    .children
                    .iter()
                    .find(|child| child.definition_id == id)
                    .map(|child| (id, child.layer_index))
            })
        })
        .collect::<HashMap<_, _>>();
    let mut out = Vec::new();
    let mut index = 0;
    while index < trace.events.len() {
        let begin = &trace.events[index];
        let Some(command) = begin
            .command
            .as_deref()
            .filter(|command| command.ends_with("MessageCommand"))
        else {
            index += 1;
            continue;
        };
        if begin.operation != "command.begin" {
            index += 1;
            continue;
        }
        let message = command
            .strip_suffix("MessageCommand")
            .expect("message command suffix was checked")
            .to_string();
        let mut calls: HashMap<&str, Vec<&NativeEvent>> = HashMap::new();
        index += 1;
        while index < trace.events.len() {
            let event = &trace.events[index];
            if event.operation == "command.end" && event.command.as_deref() == Some(command) {
                break;
            }
            if event.kind == "call"
                && let Some(actor) = event.actor.as_deref()
            {
                calls.entry(actor).or_default().push(event);
            }
            index += 1;
        }
        for (actor, events) in calls {
            let target = if paths
                .get(actor)
                .is_some_and(|path| path.ends_with("/PlayerP1"))
            {
                Some(NativeTarget::Player(0))
            } else if paths
                .get(actor)
                .is_some_and(|path| path.ends_with("/PlayerP2"))
            {
                Some(NativeTarget::Player(1))
            } else {
                layers.get(actor).copied().map(NativeTarget::Layer)
            };
            if let Some(target) = target {
                out.push(ExpectedCommand {
                    message: message.clone(),
                    target,
                    blocks: expected_blocks(&events),
                });
            }
        }
        index += 1;
    }
    out
}

fn block_matches(expected: &ExpectedBlock, actual: &SongLuaOverlayCommandBlock) -> bool {
    (expected.duration - actual.duration).abs() <= EPSILON
        && expected.easing == actual.easing.as_deref()
        && expected.alpha.is_none_or(|value| {
            actual
                .delta
                .diffuse
                .is_some_and(|diffuse| (diffuse[3] - value).abs() <= EPSILON)
        })
        && expected.zoom.is_none_or(|value| {
            actual
                .delta
                .zoom
                .is_some_and(|zoom| (zoom - value).abs() <= EPSILON)
        })
        && expected.rot_z.is_none_or(|value| {
            actual
                .delta
                .rot_z_deg
                .is_some_and(|rotation| (rotation - value).abs() <= EPSILON)
        })
}

fn compare_commands(trace: &NativeTrace, compiled: &CompiledSongLua, gaps: &mut Vec<String>) {
    for expected in trace_commands(trace) {
        let (label, commands) = match expected.target {
            NativeTarget::Layer(layer) => (
                format!("layer {layer}"),
                compiled
                    .overlays
                    .get(layer.saturating_sub(1))
                    .map(|overlay| overlay.message_commands.as_slice())
                    .unwrap_or_default(),
            ),
            NativeTarget::Player(player) => (
                format!("PlayerP{}", player + 1),
                compiled.player_actors[player].message_commands.as_slice(),
            ),
        };
        let Some(actual) = commands
            .iter()
            .find(|command| command.message == expected.message)
        else {
            gaps.push(format!(
                "missing {}MessageCommand effects on {label}",
                expected.message
            ));
            continue;
        };
        if expected.blocks.len() != actual.blocks.len()
            || !expected
                .blocks
                .iter()
                .zip(&actual.blocks)
                .all(|(expected, actual)| block_matches(expected, actual))
        {
            gaps.push(format!(
                "{}MessageCommand differs on {label}: ITGmania has {} semantic blocks, DeadSync has {:#?}",
                expected.message,
                expected.blocks.len(),
                actual.blocks
            ));
        }
    }
}

#[test]
#[ignore = "reports the current known song-Lua parity gaps"]
fn native_song_lua_semantics_match_deadsync() {
    let trace = read_trace();
    assert_eq!(trace.oracle, "itgmania_song_lua_semantic_trace");
    let compiled = compile_trace_song(&trace);
    let mut gaps = Vec::new();
    compare_layers(&trace, &compiled, &mut gaps);
    compare_timeline(&trace, &compiled, &mut gaps);
    compare_commands(&trace, &compiled, &mut gaps);
    assert!(
        gaps.is_empty(),
        "song Lua semantic parity gaps ({}):\n- {}",
        gaps.len(),
        gaps.join("\n- ")
    );
}

#[test]
fn starred_modifier_normalization_ignores_existing_options() {
    assert_eq!(
        starred_mods("Overhead, 100% Dark, *1 no dark, *2 80% stealth"),
        "*1 no dark, *2 80% stealth"
    );
}
