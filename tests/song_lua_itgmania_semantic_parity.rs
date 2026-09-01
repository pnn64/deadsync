use deadsync_assets::song_lua::{
    compile_song_lua_layers, CompiledSongLua, SongLuaCompileContext, SongLuaDifficulty,
    SongLuaOverlayCommandBlock, SongLuaOverlayKind, SongLuaPlayerContext, SongLuaSpeedMod,
};
use deadsync_simfile::song::{parse_song_meta_file, ParseSongOptions};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const TRACE_ENV: &str = "ITGMANIA_SONG_LUA_TRACE";
const SIMFILE_ENV: &str = "ITGMANIA_SONG_LUA_SIMFILE";
const DEFAULT_TRACE: &str =
    "tests/fixtures/itgmania-song-lua/Delightful Day/Delightful Day.ssc.semantic.json";
const SEMANTIC_MANIFEST: &str = "_semantic_manifest.json";
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
    timeline_tracks: Vec<NativeTimelineTrack>,
    tween_tracks: Vec<NativeTweenTrack>,
    #[serde(default)]
    draw_orders: Vec<NativeDrawOrder>,
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
struct NativeDrawOrder {
    parent_definition_id: String,
    instance: usize,
    final_children: Vec<NativeDrawChild>,
}

#[derive(Deserialize)]
struct NativeDrawChild {
    definition_id: String,
}

#[derive(Deserialize)]
struct NativeTimelineTrack {
    kind: String,
    actor: Option<String>,
    operation: String,
    samples: Vec<(u64, Option<f32>, Option<f32>, Vec<Value>, Option<Value>)>,
}

#[derive(Deserialize)]
struct NativeTweenTrack {
    actor: String,
    command: Option<String>,
    kind: String,
    easing: Option<String>,
    segments: Vec<NativeTweenSegment>,
}

#[derive(Deserialize)]
struct NativeTweenSegment {
    enqueue_seq: u64,
    duration: f32,
    #[serde(default)]
    operations: Vec<NativeTweenOperation>,
}

#[derive(Deserialize)]
struct NativeTweenOperation {
    operation: String,
    #[serde(default)]
    args: Vec<Value>,
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
    blocks: Vec<(u64, ExpectedBlock)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeTarget {
    Layer(usize),
    Player(usize),
}

#[derive(Deserialize)]
struct SemanticManifest {
    itgmania: SemanticOracle,
    simfiles: Vec<SemanticManifestEntry>,
}

#[derive(Deserialize)]
struct SemanticOracle {
    execution: String,
    launches_executable: bool,
}

#[derive(Deserialize)]
struct SemanticManifestEntry {
    fixture: PathBuf,
    status: String,
    #[serde(default)]
    runtime_errors: usize,
    #[serde(default)]
    dropped_events: usize,
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
            enabled: true,
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
    let draw_order = trace
        .draw_orders
        .iter()
        .find(|order| order.parent_definition_id == root.id && order.instance == 1);
    let mut source_children = root.children.iter().collect::<Vec<_>>();
    source_children.sort_by_key(|child| child.layer_index);
    let child_ids = draw_order
        .map(|order| {
            order
                .final_children
                .iter()
                .map(|child| child.definition_id.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            source_children
                .iter()
                .map(|child| child.definition_id.as_str())
                .collect()
        });
    let native = child_ids
        .iter()
        .filter_map(|child| definitions.get(child).copied())
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
    for track in &trace.timeline_tracks {
        for (_, beat, _, args, _) in &track.samples {
            let Some(beat) = beat else { continue };
            if track.kind == "message" {
                let Some(message) = args.first().and_then(Value::as_str) else {
                    continue;
                };
                if !compiled
                    .messages
                    .iter()
                    .any(|actual| actual.message == message && (actual.beat - beat).abs() <= 0.1)
                {
                    gaps.push(format!(
                        "missing message `{message}` near beat {beat:.3} (operation {})",
                        track.operation
                    ));
                }
            } else if track.kind == "modifier" {
                let Some(raw) = args.get(1).and_then(Value::as_str) else {
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
                        track.actor.as_deref().unwrap_or("unknown player")
                    ));
                }
            }
        }
    }
}

fn value_f32(value: Option<&Value>) -> Option<f32> {
    value.and_then(Value::as_f64).map(|value| value as f32)
}

fn expected_block(track: &NativeTweenTrack, segment: &NativeTweenSegment) -> ExpectedBlock {
    let easing = match track.easing.as_deref() {
        Some("linear") => Some("linear"),
        Some("accelerate") => Some("inQuad"),
        Some("decelerate") => Some("outQuad"),
        Some("smooth") => Some("inOutQuad"),
        _ => None,
    };
    let mut block = ExpectedBlock {
        duration: segment.duration,
        easing,
        ..ExpectedBlock::default()
    };
    for operation in &segment.operations {
        let method = operation
            .operation
            .rsplit('.')
            .next()
            .unwrap_or(&operation.operation);
        match method {
            "diffusealpha" => block.alpha = value_f32(operation.args.first()),
            "zoom" => block.zoom = value_f32(operation.args.first()),
            "addrotationz" | "rotationz" => {
                block.rot_z = value_f32(operation.args.first());
            }
            _ => {}
        }
    }
    block
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
    let mut out = Vec::<ExpectedCommand>::new();
    for track in &trace.tween_tracks {
        let Some(command) = track
            .command
            .as_deref()
            .filter(|command| command.ends_with("MessageCommand"))
        else {
            continue;
        };
        let message = command
            .strip_suffix("MessageCommand")
            .expect("message command suffix was checked")
            .to_string();
        let target = if paths
            .get(track.actor.as_str())
            .is_some_and(|path| path.ends_with("/PlayerP1"))
        {
            Some(NativeTarget::Player(0))
        } else if paths
            .get(track.actor.as_str())
            .is_some_and(|path| path.ends_with("/PlayerP2"))
        {
            Some(NativeTarget::Player(1))
        } else {
            layers
                .get(track.actor.as_str())
                .copied()
                .map(NativeTarget::Layer)
        };
        let Some(target) = target else { continue };
        let index = out
            .iter()
            .position(|item| item.message == message && item.target == target)
            .unwrap_or_else(|| {
                out.push(ExpectedCommand {
                    message: message.clone(),
                    target,
                    blocks: Vec::new(),
                });
                out.len() - 1
            });
        if matches!(track.kind.as_str(), "tween" | "sleep" | "immediate") {
            out[index].blocks.extend(
                track
                    .segments
                    .iter()
                    .map(|segment| (segment.enqueue_seq, expected_block(track, segment))),
            );
        }
    }
    for command in &mut out {
        command.blocks.sort_by_key(|(seq, _)| *seq);
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
                .all(|((_, expected), actual)| block_matches(expected, actual))
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
    assert_eq!(trace.oracle, "itgmania_song_lua_headless_semantic_trace");
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
fn semantic_fixture_manifest_is_complete_and_headless() {
    let fixture_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/itgmania-song-lua");
    let manifest: SemanticManifest = serde_json::from_slice(
        &fs::read(fixture_root.join(SEMANTIC_MANIFEST))
            .expect("missing semantic song Lua fixture manifest"),
    )
    .expect("invalid semantic song Lua fixture manifest");

    assert_eq!(manifest.itgmania.execution, "embedded_bundled_lua");
    assert!(!manifest.itgmania.launches_executable);
    assert_eq!(manifest.simfiles.len(), 42);
    for entry in manifest.simfiles {
        assert_eq!(
            entry.status, "ok",
            "incomplete fixture: {:?}",
            entry.fixture
        );
        assert_eq!(
            entry.runtime_errors, 0,
            "runtime errors: {:?}",
            entry.fixture
        );
        assert_eq!(
            entry.dropped_events, 0,
            "dropped events: {:?}",
            entry.fixture
        );
        assert!(
            fixture_root.join(&entry.fixture).is_file(),
            "missing semantic fixture {:?}",
            entry.fixture
        );
    }
}

#[test]
fn starred_modifier_normalization_ignores_existing_options() {
    assert_eq!(
        starred_mods("Overhead, 100% Dark, *1 no dark, *2 80% stealth"),
        "*1 no dark, *2 80% stealth"
    );
}
