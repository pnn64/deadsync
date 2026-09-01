use deadsync_assets::song_lua::{
    CompiledSongLua, SongLuaCompileContext, SongLuaDifficulty, SongLuaOverlayCommandBlock,
    SongLuaOverlayKind, SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaOverlayUpdateTarget,
    SongLuaOverlayUpdateValue, SongLuaPlayerContext, SongLuaSpeedMod, compile_song_lua_layers,
    overlay_state_after_blocks,
};
use deadsync_simfile::song::{ParseSongOptions, parse_song_meta_file};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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
    fixture_context: NativeFixtureContext,
    trace_until_beat: f32,
}

#[derive(Deserialize)]
struct NativeDefinition {
    id: String,
    class: String,
    name: Option<String>,
    #[serde(default)]
    children: Vec<NativeChild>,
    #[serde(default)]
    runtime_actors: Vec<String>,
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
    beat: f32,
    duration: f32,
    #[serde(default)]
    implicit: bool,
    #[serde(default)]
    operations: Vec<NativeTweenOperation>,
}

#[derive(Deserialize)]
struct NativeTweenOperation {
    seq: u64,
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

#[derive(Deserialize)]
struct NativeFixtureContext {
    beat_step: f32,
}

#[derive(Default)]
struct ExpectedBlock {
    start: f32,
    duration: f32,
    easing: Option<&'static str>,
    alpha: Option<f32>,
    visible: Option<bool>,
    x: Option<f32>,
    y: Option<f32>,
    z: Option<f32>,
    zoom: Option<f32>,
    zoom_x: Option<f32>,
    zoom_y: Option<f32>,
    rot_z: Option<f32>,
    crop_left: Option<f32>,
    crop_right: Option<f32>,
    crop_top: Option<f32>,
    crop_bottom: Option<f32>,
    sprite_state: Option<u32>,
    sleep: bool,
    queued_command: bool,
}

struct ExpectedCommand {
    message: String,
    target: NativeTarget,
    blocks: Vec<(u64, ExpectedBlock)>,
}

#[derive(Clone, PartialEq, Eq)]
enum NativeTarget {
    Actor { layer: usize, actor: String },
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

fn compile_trace_song(trace: &NativeTrace) -> (Vec<CompiledSongLua>, usize) {
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

    let compiled =
        compile_song_lua_layers(&paths, primary_index, &context).unwrap_or_else(|error| {
            panic!("DeadSync could not compile {}: {error}", simfile.display())
        });
    assert_eq!(compiled.len(), paths.len());
    (compiled, primary_index)
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

fn compare_layers(trace: &NativeTrace, compiled: &[CompiledSongLua], gaps: &mut Vec<String>) {
    let definitions = trace
        .actor_definitions
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect::<HashMap<_, _>>();
    if trace.roots.len() != compiled.len() {
        gaps.push(format!(
            "root layer count differs: ITGmania has {}, DeadSync has {}",
            trace.roots.len(),
            compiled.len()
        ));
    }
    for (layer, (root_id, compiled)) in trace.roots.iter().zip(compiled).enumerate() {
        let Some(root) = definitions.get(root_id.as_str()).copied() else {
            gaps.push(format!("native layer {layer} has no actor-definition root"));
            continue;
        };
        let mut native = Vec::new();
        collect_native_drawables(trace, root, &definitions, &mut native);
        let deadsync = compiled
            .overlays
            .iter()
            .filter(|overlay| !matches!(kind_name(&overlay.kind), "Actor" | "ActorFrame" | "Sound"))
            .map(|overlay| (kind_name(&overlay.kind), overlay.name.as_deref()))
            .collect::<Vec<_>>();
        if native != deadsync {
            let first = native
                .iter()
                .zip(&deadsync)
                .position(|(native, deadsync)| native != deadsync)
                .unwrap_or_else(|| native.len().min(deadsync.len()));
            gaps.push(format!(
                "layer {layer} drawable order differs: ITGmania has {}, DeadSync has {}; first difference at {first}: {:?} vs {:?}",
                native.len(),
                deadsync.len(),
                native.get(first),
                deadsync.get(first)
            ));
        }
    }
}

#[derive(Clone, Copy)]
struct NativeFinalRenderState {
    alpha: f32,
    visible: bool,
    wrote_alpha: bool,
    wrote_visible: bool,
}

impl Default for NativeFinalRenderState {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            visible: true,
            wrote_alpha: false,
            wrote_visible: false,
        }
    }
}

fn collect_native_drawable_definitions<'a>(
    trace: &'a NativeTrace,
    parent: &'a NativeDefinition,
    definitions: &HashMap<&'a str, &'a NativeDefinition>,
    out: &mut Vec<&'a NativeDefinition>,
) {
    let draw_order = trace
        .draw_orders
        .iter()
        .find(|order| order.parent_definition_id == parent.id && order.instance == 1);
    let mut source_children = parent.children.iter().collect::<Vec<_>>();
    source_children.sort_by_key(|child| child.layer_index);
    let children = draw_order
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
    for child in children {
        let Some(definition) = definitions.get(child).copied() else {
            continue;
        };
        if !matches!(definition.class.as_str(), "Actor" | "ActorFrame" | "Sound") {
            out.push(definition);
        }
        collect_native_drawable_definitions(trace, definition, definitions, out);
    }
}

fn native_color_alpha(args: &[Value]) -> Option<f32> {
    args.first()
        .and_then(Value::as_array)
        .and_then(|color| value_f32(color.get(3)))
        .or_else(|| value_f32(args.get(3)))
}

fn native_final_render_state(
    trace: &NativeTrace,
    definition: &NativeDefinition,
) -> NativeFinalRenderState {
    let actor = definition
        .runtime_actors
        .first()
        .map_or(definition.id.as_str(), String::as_str);
    let mut operations = trace
        .tween_tracks
        .iter()
        .filter(|track| track.actor == actor)
        .flat_map(|track| &track.segments)
        .flat_map(|segment| &segment.operations)
        .collect::<Vec<_>>();
    operations.sort_by_key(|operation| operation.seq);
    let mut state = NativeFinalRenderState::default();
    for operation in operations {
        let method = operation
            .operation
            .rsplit('.')
            .next()
            .unwrap_or(&operation.operation)
            .to_ascii_lowercase();
        match method.as_str() {
            "diffusealpha" => {
                if let Some(alpha) = value_f32(operation.args.first()) {
                    state.alpha = alpha;
                    state.wrote_alpha = true;
                }
            }
            "diffuse" => {
                if let Some(alpha) = native_color_alpha(&operation.args) {
                    state.alpha = alpha;
                    state.wrote_alpha = true;
                }
            }
            "visible" => {
                if let Some(visible) = operation.args.first().and_then(Value::as_bool) {
                    state.visible = visible;
                    state.wrote_visible = true;
                }
            }
            _ => {}
        }
    }
    state
}

fn apply_compiled_delta(
    state: SongLuaOverlayState,
    delta: SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    overlay_state_after_blocks(
        state,
        &[SongLuaOverlayCommandBlock {
            start: 0.0,
            duration: 0.0,
            easing: None,
            opt1: None,
            opt2: None,
            delta,
        }],
        0.0,
    )
}

fn compiled_final_render_state(
    compiled: &CompiledSongLua,
    overlay_index: usize,
) -> SongLuaOverlayState {
    let overlay = &compiled.overlays[overlay_index];
    let mut state = overlay.initial_state;
    let mut messages = compiled.messages.iter().collect::<Vec<_>>();
    messages.sort_by(|left, right| left.beat.total_cmp(&right.beat));
    for event in messages {
        for command in overlay
            .message_commands
            .iter()
            .filter(|command| command.message == event.message)
        {
            state = overlay_state_after_blocks(state, &command.blocks, f32::MAX);
        }
    }
    for ease in compiled
        .overlay_eases
        .iter()
        .filter(|ease| ease.overlay_index == overlay_index)
    {
        state = apply_compiled_delta(state, ease.to);
    }
    for update in compiled
        .overlay_updates
        .iter()
        .filter(|update| update.overlay_index == overlay_index)
    {
        let Some(sample) = update.samples.last() else {
            continue;
        };
        match (update.target, &sample.value) {
            (SongLuaOverlayUpdateTarget::Diffuse, SongLuaOverlayUpdateValue::Vec4(value)) => {
                state.diffuse = *value;
            }
            (SongLuaOverlayUpdateTarget::Visible, SongLuaOverlayUpdateValue::Bool(value)) => {
                state.visible = *value;
            }
            _ => {}
        }
    }
    state
}

fn compare_final_render_states(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    gaps: &mut Vec<String>,
) {
    let definitions = trace
        .actor_definitions
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect::<HashMap<_, _>>();
    for (layer, (root_id, compiled)) in trace.roots.iter().zip(compiled).enumerate() {
        let Some(root) = definitions.get(root_id.as_str()).copied() else {
            continue;
        };
        let mut native = Vec::new();
        collect_native_drawable_definitions(trace, root, &definitions, &mut native);
        let deadsync = compiled
            .overlays
            .iter()
            .enumerate()
            .filter(|(_, overlay)| {
                !matches!(kind_name(&overlay.kind), "Actor" | "ActorFrame" | "Sound")
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if native.len() != deadsync.len() {
            continue;
        }
        for (definition, overlay_index) in native.into_iter().zip(deadsync) {
            let expected = native_final_render_state(trace, definition);
            let actual = compiled_final_render_state(compiled, overlay_index);
            if expected.wrote_alpha && (expected.alpha - actual.diffuse[3]).abs() > EPSILON {
                gaps.push(format!(
                    "layer {layer} final alpha differs for {}/{}: ITGmania {:.4}, DeadSync {:.4}",
                    definition.id, definition.class, expected.alpha, actual.diffuse[3]
                ));
            }
            if expected.wrote_visible && expected.visible != actual.visible {
                gaps.push(format!(
                    "layer {layer} final visibility differs for {}/{}: ITGmania {}, DeadSync {}",
                    definition.id, definition.class, expected.visible, actual.visible
                ));
            }
        }
    }
}

fn native_update_render_writes(
    trace: &NativeTrace,
    definition: &NativeDefinition,
) -> (Vec<(f32, f32)>, Vec<(f32, bool)>) {
    let actor = definition
        .runtime_actors
        .first()
        .map_or(definition.id.as_str(), String::as_str);
    let mut alpha = Vec::<(u64, f32, f32)>::new();
    let mut visible = Vec::<(u64, f32, bool)>::new();
    for track in trace.tween_tracks.iter().filter(|track| {
        track.actor == actor
            && track.command.as_deref() == Some("UpdateCommand")
            && track.kind == "immediate"
    }) {
        for segment in &track.segments {
            for operation in &segment.operations {
                let method = operation
                    .operation
                    .rsplit('.')
                    .next()
                    .unwrap_or(&operation.operation)
                    .to_ascii_lowercase();
                match method.as_str() {
                    "diffusealpha" => {
                        if let Some(value) = value_f32(operation.args.first()) {
                            alpha.push((operation.seq, segment.beat, value));
                        }
                    }
                    "diffuse" => {
                        if let Some(value) = native_color_alpha(&operation.args) {
                            alpha.push((operation.seq, segment.beat, value));
                        }
                    }
                    "visible" => {
                        if let Some(value) = operation.args.first().and_then(Value::as_bool) {
                            visible.push((operation.seq, segment.beat, value));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    alpha.sort_by_key(|(seq, _, _)| *seq);
    visible.sort_by_key(|(seq, _, _)| *seq);
    let mut alpha_writes = Vec::<(f32, f32)>::new();
    for (_, beat, value) in alpha {
        if let Some(last) = alpha_writes.last_mut()
            && (last.0 - beat).abs() <= EPSILON
        {
            *last = (beat, value);
        } else {
            alpha_writes.push((beat, value));
        }
    }
    let mut visible_writes = Vec::<(f32, bool)>::new();
    for (_, beat, value) in visible {
        if let Some(last) = visible_writes.last_mut()
            && (last.0 - beat).abs() <= EPSILON
        {
            *last = (beat, value);
        } else {
            visible_writes.push((beat, value));
        }
    }
    (alpha_writes, visible_writes)
}

fn compiled_update_alpha_at(
    compiled: &CompiledSongLua,
    overlay_index: usize,
    beat: f32,
) -> Option<f32> {
    let samples = &compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == overlay_index
                && track.target == SongLuaOverlayUpdateTarget::Diffuse
        })?
        .samples;
    let next = samples.partition_point(|sample| sample.beat <= beat);
    let current = samples.get(next.saturating_sub(1))?;
    let SongLuaOverlayUpdateValue::Vec4(from) = current.value else {
        return None;
    };
    let Some(next) = samples.get(next) else {
        return Some(from[3]);
    };
    let SongLuaOverlayUpdateValue::Vec4(to) = next.value else {
        return Some(from[3]);
    };
    let span = next.beat - current.beat;
    if span <= f32::EPSILON {
        return Some(to[3]);
    }
    let t = ((beat - current.beat) / span).clamp(0.0, 1.0);
    Some((to[3] - from[3]).mul_add(t, from[3]))
}

fn compiled_update_visibility_at(
    compiled: &CompiledSongLua,
    overlay_index: usize,
    beat: f32,
) -> Option<bool> {
    let samples = &compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == overlay_index
                && track.target == SongLuaOverlayUpdateTarget::Visible
        })?
        .samples;
    let next = samples.partition_point(|sample| sample.beat <= beat);
    let SongLuaOverlayUpdateValue::Bool(value) = samples.get(next.saturating_sub(1))?.value else {
        return None;
    };
    Some(value)
}

fn persistence_probes<T: Copy>(
    writes: &[(f32, T)],
    beat_step: f32,
    end_beat: f32,
) -> Vec<(f32, T)> {
    writes
        .iter()
        .enumerate()
        .filter_map(|(index, &(beat, value))| {
            let next = writes.get(index + 1).map_or(end_beat, |write| write.0);
            let probe = beat + beat_step;
            (probe < next - EPSILON && probe <= end_beat + EPSILON).then_some((probe, value))
        })
        .collect()
}

fn compare_update_render_persistence(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    gaps: &mut Vec<String>,
) {
    let definitions = trace
        .actor_definitions
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect::<HashMap<_, _>>();
    for (layer, (root_id, compiled)) in trace.roots.iter().zip(compiled).enumerate() {
        let Some(root) = definitions.get(root_id.as_str()).copied() else {
            continue;
        };
        let mut native = Vec::new();
        collect_native_drawable_definitions(trace, root, &definitions, &mut native);
        let deadsync = compiled
            .overlays
            .iter()
            .enumerate()
            .filter(|(_, overlay)| {
                !matches!(kind_name(&overlay.kind), "Actor" | "ActorFrame" | "Sound")
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if native.len() != deadsync.len() {
            continue;
        }
        for (definition, overlay_index) in native.into_iter().zip(deadsync) {
            let (alpha_writes, visible_writes) = native_update_render_writes(trace, definition);
            for (beat, expected) in persistence_probes(
                &alpha_writes,
                trace.fixture_context.beat_step,
                trace.trace_until_beat,
            ) {
                let Some(actual) = compiled_update_alpha_at(compiled, overlay_index, beat) else {
                    continue;
                };
                if (expected - actual).abs() > 0.03 {
                    gaps.push(format!(
                        "layer {layer} alpha persistence differs for {}/{} at beat {beat:.3}: ITGmania {expected:.4}, DeadSync {actual:.4}",
                        definition.id, definition.class
                    ));
                }
            }
            for (beat, expected) in persistence_probes(
                &visible_writes,
                trace.fixture_context.beat_step,
                trace.trace_until_beat,
            ) {
                let Some(actual) = compiled_update_visibility_at(compiled, overlay_index, beat)
                else {
                    continue;
                };
                if expected != actual {
                    gaps.push(format!(
                        "layer {layer} visibility persistence differs for {}/{} at beat {beat:.3}: ITGmania {expected}, DeadSync {actual}",
                        definition.id, definition.class
                    ));
                }
            }
        }
    }
}

fn collect_native_drawables<'a>(
    trace: &'a NativeTrace,
    parent: &'a NativeDefinition,
    definitions: &HashMap<&'a str, &'a NativeDefinition>,
    out: &mut Vec<(&'a str, Option<&'a str>)>,
) {
    let draw_order = trace
        .draw_orders
        .iter()
        .find(|order| order.parent_definition_id == parent.id && order.instance == 1);
    let mut source_children = parent.children.iter().collect::<Vec<_>>();
    source_children.sort_by_key(|child| child.layer_index);
    let children = draw_order
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
    for child in children {
        let Some(definition) = definitions.get(child).copied() else {
            continue;
        };
        if !matches!(definition.class.as_str(), "Actor" | "ActorFrame" | "Sound") {
            out.push((definition.class.as_str(), definition.name.as_deref()));
        }
        collect_native_drawables(trace, definition, definitions, out);
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
    let beat_epsilon = trace.fixture_context.beat_step + EPSILON;
    for track in &trace.timeline_tracks {
        for (_, beat, _, args, _) in &track.samples {
            let Some(beat) = beat else { continue };
            if track.kind == "message" {
                let Some(message) = args.first().and_then(Value::as_str) else {
                    continue;
                };
                let has_listener = compiled
                    .overlays
                    .iter()
                    .flat_map(|actor| &actor.message_commands)
                    .chain(
                        compiled
                            .player_actors
                            .iter()
                            .flat_map(|actor| &actor.message_commands),
                    )
                    .chain(&compiled.song_foreground.message_commands)
                    .any(|command| command.message == message);
                if !has_listener {
                    continue;
                }
                if !compiled.messages.iter().any(|actual| {
                    actual.message == message && (actual.beat - beat).abs() <= beat_epsilon
                }) {
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
                    (actual.start - beat).abs() <= beat_epsilon
                        && starred_mods(&actual.mods) == wanted
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

fn value_u32(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn expected_block(track: &NativeTweenTrack, segment: &NativeTweenSegment) -> ExpectedBlock {
    let easing = match track.easing.as_deref() {
        Some("linear") => Some("linear"),
        Some("accelerate") => Some("inQuad"),
        Some("decelerate") => Some("outQuad"),
        Some("smooth") => Some("inOutQuad"),
        Some("spring") => Some("outElastic"),
        Some("bouncebegin") => Some("inBounce"),
        Some("bounceend") => Some("outBounce"),
        _ => None,
    };
    let mut block = ExpectedBlock {
        duration: segment.duration,
        easing,
        sleep: track.kind == "sleep",
        queued_command: track.kind == "command",
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
            "diffuse" => block.alpha = value_f32(operation.args.get(3)),
            "visible" => block.visible = operation.args.first().and_then(Value::as_bool),
            "x" => block.x = value_f32(operation.args.first()),
            "y" => block.y = value_f32(operation.args.first()),
            "z" => block.z = value_f32(operation.args.first()),
            "zoom" => block.zoom = value_f32(operation.args.first()),
            "zoomx" => block.zoom_x = value_f32(operation.args.first()),
            "zoomy" => block.zoom_y = value_f32(operation.args.first()),
            "addrotationz" | "rotationz" => {
                block.rot_z = value_f32(operation.args.first());
            }
            "cropleft" => block.crop_left = value_f32(operation.args.first()),
            "cropright" => block.crop_right = value_f32(operation.args.first()),
            "croptop" => block.crop_top = value_f32(operation.args.first()),
            "cropbottom" => block.crop_bottom = value_f32(operation.args.first()),
            "setstate" => block.sprite_state = value_u32(operation.args.first()),
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
    let parents = trace
        .actor_definitions
        .iter()
        .flat_map(|parent| {
            parent
                .children
                .iter()
                .map(move |child| (child.definition_id.as_str(), parent.id.as_str()))
        })
        .collect::<HashMap<_, _>>();
    let root_layers = trace
        .roots
        .iter()
        .enumerate()
        .map(|(index, root)| (root.as_str(), index))
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
            let mut ancestor = track.actor.as_str();
            let layer = loop {
                if let Some(layer) = root_layers.get(ancestor).copied() {
                    break Some(layer);
                }
                let Some(parent) = parents.get(ancestor).copied() else {
                    break None;
                };
                ancestor = parent;
            };
            layer.map(|layer| NativeTarget::Actor {
                layer,
                actor: track.actor.clone(),
            })
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
        if let Some(first_beat) = track.segments.first().map(|segment| segment.beat) {
            out[index].blocks.extend(
                track
                    .segments
                    .iter()
                    .take_while(|segment| (segment.beat - first_beat).abs() <= EPSILON)
                    .filter(|segment| !segment.implicit)
                    .map(|segment| (segment.enqueue_seq, expected_block(track, segment))),
            );
        }
    }
    for command in &mut out {
        command.blocks.sort_by_key(|(seq, _)| *seq);
        let mut start = 0.0;
        command.blocks.retain_mut(|(_, block)| {
            if block.sleep {
                start += block.duration;
                return false;
            }
            if block.queued_command {
                return false;
            }
            block.start = start;
            start += block.duration;
            expected_block_has_effect(block)
        });
    }
    out.retain(|command| !command.blocks.is_empty());
    out
}

fn expected_block_has_effect(block: &ExpectedBlock) -> bool {
    block.alpha.is_some()
        || block.visible.is_some()
        || block.x.is_some()
        || block.y.is_some()
        || block.z.is_some()
        || block.zoom.is_some()
        || block.zoom_x.is_some()
        || block.zoom_y.is_some()
        || block.rot_z.is_some()
        || block.crop_left.is_some()
        || block.crop_right.is_some()
        || block.crop_top.is_some()
        || block.crop_bottom.is_some()
        || block.sprite_state.is_some()
}

fn option_f32_matches(expected: Option<f32>, actual: Option<f32>) -> bool {
    expected
        .is_none_or(|expected| actual.is_some_and(|actual| (actual - expected).abs() <= EPSILON))
}

fn block_matches(expected: &ExpectedBlock, actual: &SongLuaOverlayCommandBlock) -> bool {
    (expected.start - actual.start).abs() <= EPSILON
        && (expected.duration - actual.duration).abs() <= EPSILON
        && expected.easing == actual.easing.as_deref()
        && option_f32_matches(
            expected.alpha,
            actual.delta.diffuse.map(|diffuse| diffuse[3]),
        )
        && expected
            .visible
            .is_none_or(|value| actual.delta.visible == Some(value))
        && option_f32_matches(expected.x, actual.delta.x)
        && option_f32_matches(expected.y, actual.delta.y)
        && option_f32_matches(expected.z, actual.delta.z)
        && option_f32_matches(expected.zoom, actual.delta.zoom)
        && option_f32_matches(expected.zoom_x, actual.delta.zoom_x)
        && option_f32_matches(expected.zoom_y, actual.delta.zoom_y)
        && option_f32_matches(expected.rot_z, actual.delta.rot_z_deg)
        && option_f32_matches(expected.crop_left, actual.delta.cropleft)
        && option_f32_matches(expected.crop_right, actual.delta.cropright)
        && option_f32_matches(expected.crop_top, actual.delta.croptop)
        && option_f32_matches(expected.crop_bottom, actual.delta.cropbottom)
        && expected
            .sprite_state
            .is_none_or(|value| actual.delta.sprite_state_index == Some(value))
}

fn command_matches(expected: &ExpectedCommand, actual: &[SongLuaOverlayCommandBlock]) -> bool {
    let mut actual = actual.iter();
    expected.blocks.iter().all(|(_, expected)| {
        actual
            .by_ref()
            .any(|actual| block_matches(expected, actual))
    })
}

fn expected_blocks_summary(expected: &ExpectedCommand) -> String {
    expected
        .blocks
        .iter()
        .map(|(_, block)| {
            format!(
                "({:.3}+{:.3},{:?},a={:?},v={:?},x={:?},y={:?},z={:?},zx={:?},zy={:?},r={:?},cl={:?},cr={:?},s={:?})",
                block.start,
                block.duration,
                block.easing,
                block.alpha,
                block.visible,
                block.x,
                block.y,
                block.zoom,
                block.zoom_x,
                block.zoom_y,
                block.rot_z,
                block.crop_left,
                block.crop_right,
                block.sprite_state
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn actual_blocks_summary(actual: &[SongLuaOverlayCommandBlock]) -> String {
    actual
        .iter()
        .map(|block| {
            format!(
                "({:.3}+{:.3},{:?},a={:?},v={:?},x={:?},y={:?},z={:?},zx={:?},zy={:?},r={:?},cl={:?},cr={:?},s={:?})",
                block.start,
                block.duration,
                block.easing,
                block.delta.diffuse.map(|color| color[3]),
                block.delta.visible,
                block.delta.x,
                block.delta.y,
                block.delta.zoom,
                block.delta.zoom_x,
                block.delta.zoom_y,
                block.delta.rot_z_deg
                ,block.delta.cropleft,
                block.delta.cropright,
                block.delta.sprite_state_index
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn compare_commands(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    primary_index: usize,
    gaps: &mut Vec<String>,
) {
    let mut used = HashSet::new();
    let mut missing = HashMap::<String, Vec<String>>::new();
    for expected in trace_commands(trace) {
        let (label, candidates) = match &expected.target {
            NativeTarget::Actor { layer, actor } => (
                format!("actor {actor} in layer {layer}"),
                compiled
                    .get(*layer)
                    .into_iter()
                    .flat_map(|compiled| compiled.overlays.iter().enumerate())
                    .flat_map(|(overlay, actor)| {
                        actor
                            .message_commands
                            .iter()
                            .enumerate()
                            .map(move |(command, value)| ((*layer, overlay, command), value))
                    })
                    .collect::<Vec<_>>(),
            ),
            NativeTarget::Player(player) => (
                format!("PlayerP{}", player + 1),
                compiled
                    .get(primary_index)
                    .into_iter()
                    .flat_map(|compiled| {
                        compiled.player_actors[*player]
                            .message_commands
                            .iter()
                            .enumerate()
                    })
                    .map(|(command, value)| ((usize::MAX, *player, command), value))
                    .collect::<Vec<_>>(),
            ),
        };
        if let Some((key, _)) = candidates.iter().find(|(key, command)| {
            !used.contains(key)
                && command.message == expected.message
                && command_matches(&expected, &command.blocks)
        }) {
            used.insert(*key);
            continue;
        }
        let Some((_, actual)) = candidates
            .iter()
            .find(|(_, command)| command.message == expected.message)
        else {
            missing.entry(expected.message).or_default().push(label);
            continue;
        };
        gaps.push(format!(
            "{}MessageCommand differs on {label}: ITGmania [{}], DeadSync [{}]",
            expected.message,
            expected_blocks_summary(&expected),
            actual_blocks_summary(&actual.blocks)
        ));
    }
    for (message, targets) in missing {
        let dynamic = compiled.iter().any(|compiled| {
            compiled
                .info
                .skipped_message_command_captures
                .iter()
                .any(|detail| {
                    detail.contains(&format!(
                        "{message}MessageCommand changes cross-actor targets or effects"
                    ))
                })
        });
        if dynamic {
            gaps.push(format!(
                "stateful {message}MessageCommand cross-actor effects are not compiled: ITGmania affected {} actors ({})",
                targets.len(),
                targets.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
            ));
        } else {
            gaps.extend(
                targets
                    .into_iter()
                    .map(|target| format!("missing {message}MessageCommand effects on {target}")),
            );
        }
    }
}

fn compare_compile_info(compiled: &[CompiledSongLua], gaps: &mut Vec<String>) {
    for (layer, compiled) in compiled.iter().enumerate() {
        gaps.extend(
            compiled
                .info
                .unsupported_function_ease_captures
                .iter()
                .map(|detail| format!("layer {layer} unsupported function ease: {detail}")),
        );
        gaps.extend(
            compiled
                .info
                .unsupported_function_action_captures
                .iter()
                .map(|detail| format!("layer {layer} unsupported function action: {detail}")),
        );
        gaps.extend(
            compiled
                .info
                .unsupported_perframe_captures
                .iter()
                .map(|detail| format!("layer {layer} unsupported perframe: {detail}")),
        );
        gaps.extend(
            compiled
                .info
                .skipped_message_command_captures
                .iter()
                .map(|detail| format!("layer {layer} skipped message command: {detail}")),
        );
    }
}

#[test]
#[ignore = "reports the current known song-Lua parity gaps"]
fn native_song_lua_semantics_match_deadsync() {
    let trace = read_trace();
    assert_eq!(trace.oracle, "itgmania_song_lua_headless_semantic_trace");
    let (compiled, primary_index) = compile_trace_song(&trace);
    eprintln!(
        "compiled {} layer(s): {} overlays, {} overlay eases, {} overlay update tracks, {} beat mods, {} messages; unsupported: {} function eases, {} function actions, {} perframes, {} skipped message commands",
        compiled.len(),
        compiled
            .iter()
            .map(|layer| layer.overlays.len())
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.overlay_eases.len())
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.overlay_updates.len())
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.beat_mods.len())
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.messages.len())
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.info.unsupported_function_eases)
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.info.unsupported_function_actions)
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.info.unsupported_perframes)
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.info.skipped_message_command_captures.len())
            .sum::<usize>(),
    );
    let mut gaps = Vec::new();
    compare_compile_info(&compiled, &mut gaps);
    compare_layers(&trace, &compiled, &mut gaps);
    compare_final_render_states(&trace, &compiled, &mut gaps);
    compare_update_render_persistence(&trace, &compiled, &mut gaps);
    compare_timeline(&trace, &compiled[primary_index], &mut gaps);
    compare_commands(&trace, &compiled, primary_index, &mut gaps);
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
