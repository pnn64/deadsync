use deadlib_present::anim::EffectMode;
use deadsync_assets::song_lua::{
    CompiledSongLua, SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget,
    SongLuaOverlayCommandBlock, SongLuaOverlayKind, SongLuaOverlayState, SongLuaOverlayStateDelta,
    SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue, SongLuaPlayerContext, SongLuaSpanMode,
    SongLuaSpeedMod, SongLuaStatefulMessageWrite, SongLuaTimeUnit, compile_song_lua_layers,
    overlay_state_after_blocks, parse_song_timing_bpms, song_elapsed_seconds_at,
};
use deadsync_simfile::song::{ParseSongOptions, parse_song_meta_file};
use deadsync_song_lua::song_beat_at_elapsed_seconds;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const TRACE_ENV: &str = "ITGMANIA_SONG_LUA_TRACE";
const SIMFILE_ENV: &str = "ITGMANIA_SONG_LUA_SIMFILE";
const DEFAULT_TRACE: &str =
    "tests/fixtures/itgmania-song-lua/Delightful Day/Delightful Day.ssc.semantic.json";
const STEP_YOUR_GAME_UP_TRACE: &str = "tests/fixtures/itgmania-song-lua/Step Your Game Up (Director's Cut)/stepyourgameup.ssc.semantic.json";
const CUPHEAD_TRACE: &str =
    "tests/fixtures/itgmania-song-lua/Cuphead [TaroNuke]/botanic.sm.semantic.json";
const SEMANTIC_MANIFEST: &str = "_semantic_manifest.json";
const EPSILON: f32 = 0.002;

#[path = "song_lua_itgmania_semantic_parity/whole_song_archives.rs"]
mod whole_song_archives;

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
    operation_tracks: Vec<NativeOperationTrack>,
    #[serde(default)]
    draw_orders: Vec<NativeDrawOrder>,
    #[serde(default)]
    external_actors: Vec<NativeExternalActor>,
    #[serde(default)]
    player_render_tracks: Vec<NativePlayerRenderTrack>,
    #[serde(default)]
    projected_vertex_tracks: Vec<NativeProjectedVertexTrack>,
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
struct NativeExternalActor {
    id: String,
    path: String,
    class: String,
}

#[derive(Deserialize)]
struct NativePlayerRenderTrack {
    player: usize,
    path: String,
    samples: Vec<(f32, f32, bool, bool, bool, Vec<String>)>,
}

#[derive(Deserialize)]
struct NativeProjectedVertexTrack {
    actor: String,
    definition_id: Option<String>,
    texture: String,
    texture_size: [f32; 2],
    camera_actor: String,
    sample_layout: Vec<String>,
    samples: Vec<Value>,
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
struct NativeOperationTrack {
    actor: String,
    operation: String,
    samples: Vec<(u64, f32, f32, Vec<Value>)>,
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
    zoom_z: Option<f32>,
    rot_x: Option<f32>,
    rot_y: Option<f32>,
    rot_z: Option<f32>,
    skew_x: Option<f32>,
    skew_y: Option<f32>,
    fov: Option<f32>,
    vanishpoint: Option<[f32; 2]>,
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
    read_trace_file(&path)
}

fn read_trace_file(path: &Path) -> NativeTrace {
    serde_json::from_slice(
        &fs::read(path).unwrap_or_else(|error| {
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

fn compile_trace_song(trace: &NativeTrace) -> (Vec<CompiledSongLua>, usize, SongLuaCompileContext) {
    let simfile = locate_simfile(trace);
    compile_trace_song_at(trace, &simfile)
}

fn compile_trace_song_at(
    trace: &NativeTrace,
    simfile: &Path,
) -> (Vec<CompiledSongLua>, usize, SongLuaCompileContext) {
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
    let timing_bpms = parse_song_timing_bpms(&song.normalized_bpms);
    if !timing_bpms.is_empty() {
        context.song_timing_bpms = timing_bpms;
    }
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
    (compiled, primary_index, context)
}

fn kind_name(kind: &SongLuaOverlayKind) -> &'static str {
    match kind {
        SongLuaOverlayKind::Actor => "Actor",
        SongLuaOverlayKind::ActorFrame => "ActorFrame",
        SongLuaOverlayKind::UpdateTracks { .. } => "UpdateTracks",
        SongLuaOverlayKind::ActorFrameTexture { .. } => "ActorFrameTexture",
        SongLuaOverlayKind::ActorProxy { .. } => "ActorProxy",
        // An AFT-backed sprite is still a native Sprite. AftSprite is only
        // DeadSync's internal texture-source specialization.
        SongLuaOverlayKind::AftSprite { .. } => "Sprite",
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

#[derive(Clone)]
struct NativeRenderWrite {
    seq: u64,
    beat: f32,
    target: SongLuaOverlayUpdateTarget,
    value: SongLuaOverlayUpdateValue,
}

fn native_render_values(
    operation: &NativeTweenOperation,
) -> Vec<(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)> {
    use SongLuaOverlayUpdateTarget as Target;
    use SongLuaOverlayUpdateValue as UpdateValue;

    let method = operation
        .operation
        .rsplit('.')
        .next()
        .unwrap_or(&operation.operation)
        .to_ascii_lowercase();
    let number = |index| value_f32(operation.args.get(index));
    let scalar = |target, index| {
        number(index)
            .map(|value| vec![(target, UpdateValue::F32(value))])
            .unwrap_or_default()
    };
    match method.as_str() {
        "x" => scalar(Target::X, 0),
        "y" => scalar(Target::Y, 0),
        "z" => scalar(Target::Z, 0),
        "xy" => match (number(0), number(1)) {
            (Some(x), Some(y)) => vec![
                (Target::X, UpdateValue::F32(x)),
                (Target::Y, UpdateValue::F32(y)),
            ],
            _ => Vec::new(),
        },
        "zoom" => number(0).map_or_else(Vec::new, |value| {
            [Target::Zoom, Target::ZoomX, Target::ZoomY, Target::ZoomZ]
                .into_iter()
                .map(|target| (target, UpdateValue::F32(value)))
                .collect()
        }),
        "zoomx" => scalar(Target::ZoomX, 0),
        "zoomy" => scalar(Target::ZoomY, 0),
        "zoomz" => scalar(Target::ZoomZ, 0),
        "basezoom" => number(0).map_or_else(Vec::new, |value| {
            [
                Target::BaseZoom,
                Target::BaseZoomX,
                Target::BaseZoomY,
                Target::BaseZoomZ,
            ]
            .into_iter()
            .map(|target| (target, UpdateValue::F32(value)))
            .collect()
        }),
        "basezoomx" => scalar(Target::BaseZoomX, 0),
        "basezoomy" => scalar(Target::BaseZoomY, 0),
        "basezoomz" => scalar(Target::BaseZoomZ, 0),
        "rotationx" | "baserotationx" => scalar(Target::RotationX, 0),
        "rotationy" | "baserotationy" => scalar(Target::RotationY, 0),
        "rotationz" | "baserotationz" => scalar(Target::RotationZ, 0),
        "skewx" => scalar(Target::SkewX, 0),
        "skewy" => scalar(Target::SkewY, 0),
        "fov" | "setfov" => scalar(Target::Fov, 0),
        "vanishpoint" => match (number(0), number(1)) {
            (Some(x), Some(y)) => vec![(Target::Vanishpoint, UpdateValue::Vec2([x, y]))],
            _ => Vec::new(),
        },
        "halign" => scalar(Target::HAlign, 0),
        "valign" => scalar(Target::VAlign, 0),
        "cropleft" => scalar(Target::CropLeft, 0),
        "cropright" => scalar(Target::CropRight, 0),
        "croptop" => scalar(Target::CropTop, 0),
        "cropbottom" => scalar(Target::CropBottom, 0),
        "fadeleft" => scalar(Target::FadeLeft, 0),
        "faderight" => scalar(Target::FadeRight, 0),
        "fadetop" => scalar(Target::FadeTop, 0),
        "fadebottom" => scalar(Target::FadeBottom, 0),
        "effectperiod" => scalar(Target::EffectPeriod, 0),
        "effectoffset" => scalar(Target::EffectOffset, 0),
        "effectmagnitude" => match (number(0), number(1), number(2)) {
            (Some(x), Some(y), Some(z)) => {
                vec![(Target::EffectMagnitude, UpdateValue::Vec3([x, y, z]))]
            }
            _ => Vec::new(),
        },
        "vibrate" => vec![(Target::Vibrate, UpdateValue::Bool(true))],
        "stopeffect" => vec![
            (Target::Vibrate, UpdateValue::Bool(false)),
            (
                Target::EffectMode,
                UpdateValue::EffectMode(EffectMode::None),
            ),
        ],
        "spin" => vec![(
            Target::EffectMode,
            UpdateValue::EffectMode(EffectMode::Spin),
        )],
        "bob" => vec![(Target::EffectMode, UpdateValue::EffectMode(EffectMode::Bob))],
        "bounce" => vec![(
            Target::EffectMode,
            UpdateValue::EffectMode(EffectMode::Bounce),
        )],
        "wag" => vec![(Target::EffectMode, UpdateValue::EffectMode(EffectMode::Wag))],
        "pulse" => vec![(
            Target::EffectMode,
            UpdateValue::EffectMode(EffectMode::Pulse),
        )],
        "diffuseramp" => vec![(
            Target::EffectMode,
            UpdateValue::EffectMode(EffectMode::DiffuseRamp),
        )],
        "diffuseshift" => vec![(
            Target::EffectMode,
            UpdateValue::EffectMode(EffectMode::DiffuseShift),
        )],
        "glowshift" => vec![(
            Target::EffectMode,
            UpdateValue::EffectMode(EffectMode::GlowShift),
        )],
        "zoomto" | "scaletoclipped" => match (number(0), number(1)) {
            (Some(width), Some(height)) => {
                vec![(Target::Size, UpdateValue::Vec2([width, height]))]
            }
            _ => Vec::new(),
        },
        "visible" => operation
            .args
            .first()
            .and_then(Value::as_bool)
            .map(|value| vec![(Target::Visible, UpdateValue::Bool(value))])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn native_update_render_writes_all(
    trace: &NativeTrace,
    definition: &NativeDefinition,
) -> Vec<NativeRenderWrite> {
    let actor = definition
        .runtime_actors
        .first()
        .map_or(definition.id.as_str(), String::as_str);
    let mut writes = trace
        .tween_tracks
        .iter()
        .filter(|track| {
            track.actor == actor
                && track.command.as_deref() == Some("UpdateCommand")
                && track.kind == "immediate"
        })
        .flat_map(|track| &track.segments)
        .flat_map(|segment| {
            segment.operations.iter().flat_map(move |operation| {
                native_render_values(operation)
                    .into_iter()
                    .map(move |(target, value)| NativeRenderWrite {
                        seq: operation.seq,
                        beat: segment.beat,
                        target,
                        value,
                    })
            })
        })
        .collect::<Vec<_>>();
    writes.sort_by_key(|write| write.seq);
    let mut merged = Vec::<NativeRenderWrite>::with_capacity(writes.len());
    for write in writes {
        if let Some(index) = merged.iter().position(|current| {
            current.target == write.target && (current.beat - write.beat).abs() <= EPSILON
        }) {
            merged[index] = write;
        } else {
            merged.push(write);
        }
    }
    merged.sort_by(|left, right| left.beat.total_cmp(&right.beat));
    merged
}

fn update_value_lerp(
    from: &SongLuaOverlayUpdateValue,
    to: &SongLuaOverlayUpdateValue,
    t: f32,
) -> SongLuaOverlayUpdateValue {
    use SongLuaOverlayUpdateValue as UpdateValue;
    match (from, to) {
        (UpdateValue::F32(from), UpdateValue::F32(to)) => {
            UpdateValue::F32((to - from).mul_add(t, *from))
        }
        (UpdateValue::Vec2(from), UpdateValue::Vec2(to)) => UpdateValue::Vec2([
            (to[0] - from[0]).mul_add(t, from[0]),
            (to[1] - from[1]).mul_add(t, from[1]),
        ]),
        (UpdateValue::Vec3(from), UpdateValue::Vec3(to)) => UpdateValue::Vec3([
            (to[0] - from[0]).mul_add(t, from[0]),
            (to[1] - from[1]).mul_add(t, from[1]),
            (to[2] - from[2]).mul_add(t, from[2]),
        ]),
        _ => from.clone(),
    }
}

fn compiled_update_value_at(
    compiled: &CompiledSongLua,
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    beat: f32,
) -> Option<SongLuaOverlayUpdateValue> {
    let samples = &compiled
        .overlay_updates
        .iter()
        .find(|track| track.overlay_index == overlay_index && track.target == target)?
        .samples;
    let next_index = samples.partition_point(|sample| sample.beat <= beat);
    if next_index == 0 {
        return None;
    }
    let current = &samples[next_index - 1];
    let Some(next) = samples.get(next_index) else {
        return Some(current.value.clone());
    };
    let span = next.beat - current.beat;
    // The native oracle serializes its 60 Hz clock as f64 while runtime tracks
    // store beat positions as f32.  Compare the same frame directly when the
    // two representations straddle it. Dense tracks contain one sample per
    // update frame; sparse tween tracks must retain interpolation semantics.
    let nearest = [current, next].into_iter().min_by(|left, right| {
        (left.beat - beat)
            .abs()
            .total_cmp(&(right.beat - beat).abs())
    })?;
    let frame_epsilon = if span <= 0.125 {
        (span * 0.51).max(EPSILON)
    } else {
        EPSILON
    };
    if (nearest.beat - beat).abs() <= frame_epsilon {
        return Some(nearest.value.clone());
    }
    if span <= f32::EPSILON {
        return Some(next.value.clone());
    }
    Some(update_value_lerp(
        &current.value,
        &next.value,
        ((beat - current.beat) / span).clamp(0.0, 1.0),
    ))
}

fn overlay_state_render_value(
    state: &SongLuaOverlayState,
    target: SongLuaOverlayUpdateTarget,
) -> Option<SongLuaOverlayUpdateValue> {
    use SongLuaOverlayUpdateTarget as Target;
    use SongLuaOverlayUpdateValue as UpdateValue;
    Some(match target {
        Target::X => UpdateValue::F32(state.x),
        Target::Y => UpdateValue::F32(state.y),
        Target::Z => UpdateValue::F32(state.z),
        Target::Zoom => UpdateValue::F32(state.zoom),
        Target::ZoomX => UpdateValue::F32(state.zoom_x),
        Target::ZoomY => UpdateValue::F32(state.zoom_y),
        Target::ZoomZ => UpdateValue::F32(state.zoom_z),
        Target::BaseZoom => UpdateValue::F32(state.basezoom),
        Target::BaseZoomX => UpdateValue::F32(state.basezoom_x),
        Target::BaseZoomY => UpdateValue::F32(state.basezoom_y),
        Target::BaseZoomZ => UpdateValue::F32(state.basezoom_z),
        Target::RotationX => UpdateValue::F32(state.rot_x_deg),
        Target::RotationY => UpdateValue::F32(state.rot_y_deg),
        Target::RotationZ => UpdateValue::F32(state.rot_z_deg),
        Target::SkewX => UpdateValue::F32(state.skew_x),
        Target::SkewY => UpdateValue::F32(state.skew_y),
        Target::Fov => state.fov.map_or(UpdateValue::None, UpdateValue::F32),
        Target::Vanishpoint => state
            .vanishpoint
            .map_or(UpdateValue::None, UpdateValue::Vec2),
        Target::HAlign => UpdateValue::F32(state.halign),
        Target::VAlign => UpdateValue::F32(state.valign),
        Target::Visible => UpdateValue::Bool(state.visible),
        Target::CropLeft => UpdateValue::F32(state.cropleft),
        Target::CropRight => UpdateValue::F32(state.cropright),
        Target::CropTop => UpdateValue::F32(state.croptop),
        Target::CropBottom => UpdateValue::F32(state.cropbottom),
        Target::FadeLeft => UpdateValue::F32(state.fadeleft),
        Target::FadeRight => UpdateValue::F32(state.faderight),
        Target::FadeTop => UpdateValue::F32(state.fadetop),
        Target::FadeBottom => UpdateValue::F32(state.fadebottom),
        Target::Vibrate => UpdateValue::Bool(state.vibrate),
        Target::EffectMagnitude => UpdateValue::Vec3(state.effect_magnitude),
        Target::EffectMode => UpdateValue::EffectMode(state.effect_mode),
        Target::EffectPeriod => UpdateValue::F32(state.effect_period),
        Target::EffectOffset => UpdateValue::F32(state.effect_offset),
        Target::Size => state.size.map_or(UpdateValue::None, UpdateValue::Vec2),
        _ => return None,
    })
}

fn render_value_matches(
    expected: &SongLuaOverlayUpdateValue,
    actual: &SongLuaOverlayUpdateValue,
) -> bool {
    use SongLuaOverlayUpdateValue as UpdateValue;
    match (expected, actual) {
        (UpdateValue::F32(expected), UpdateValue::F32(actual)) => (expected - actual).abs() <= 0.03,
        (UpdateValue::Vec2(expected), UpdateValue::Vec2(actual)) => expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| (expected - actual).abs() <= 0.03),
        (UpdateValue::Vec3(expected), UpdateValue::Vec3(actual)) => expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| (expected - actual).abs() <= 0.03),
        _ => expected == actual,
    }
}

fn collect_native_overlay_definitions<'a>(
    parent: &'a NativeDefinition,
    definitions: &HashMap<&'a str, &'a NativeDefinition>,
    out: &mut Vec<&'a NativeDefinition>,
) {
    let mut children = parent.children.iter().collect::<Vec<_>>();
    children.sort_by_key(|child| child.layer_index);
    for child in children {
        let Some(definition) = definitions.get(child.definition_id.as_str()).copied() else {
            continue;
        };
        if definition.class != "Actor" || !definition.children.is_empty() {
            out.push(definition);
        }
        collect_native_overlay_definitions(definition, definitions, out);
    }
}

fn compare_update_render_values(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
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
        collect_native_overlay_definitions(root, &definitions, &mut native);
        let native_len = native.len();
        let pairs = if native_len == compiled.overlays.len() {
            native
                .into_iter()
                .enumerate()
                .map(|(index, definition)| (index, definition))
                .collect::<Vec<_>>()
        } else {
            let mut native_drawables = Vec::new();
            collect_native_drawable_definitions(trace, root, &definitions, &mut native_drawables);
            let compiled_drawables = compiled
                .overlays
                .iter()
                .enumerate()
                .filter(|(_, overlay)| {
                    !matches!(kind_name(&overlay.kind), "Actor" | "ActorFrame" | "Sound")
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if native_drawables.len() == compiled_drawables.len() {
                compiled_drawables
                    .into_iter()
                    .zip(native_drawables)
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        };
        if pairs.is_empty() && native_len != 0 {
            gaps.push(format!(
                "layer {layer} update comparison topology differs: ITGmania has {} non-root actors, DeadSync has {} overlays",
                native_len,
                compiled.overlays.len()
            ));
            continue;
        }
        for (overlay_index, definition) in pairs {
            for write in native_update_render_writes_all(trace, definition) {
                let exact_ease_is_authoritative = compiled.overlay_eases.iter().any(|ease| {
                    if ease.overlay_index != overlay_index
                        || ease.unit != SongLuaTimeUnit::Beat
                        || (!ease.from.has_update_target(write.target)
                            && !ease.to.has_update_target(write.target))
                    {
                        return false;
                    }
                    let end = match ease.span_mode {
                        SongLuaSpanMode::Len => ease.start + ease.limit,
                        SongLuaSpanMode::End => ease.limit,
                    };
                    let sustain_end = end + ease.sustain.unwrap_or(0.0).max(0.0);
                    write.beat + EPSILON >= ease.start && write.beat < sustain_end - EPSILON
                });
                if exact_ease_is_authoritative {
                    continue;
                }
                let actual =
                    compiled_update_value_at(compiled, overlay_index, write.target, write.beat)
                        .or_else(|| {
                            let seconds = song_elapsed_seconds_at(write.beat, context);
                            let state = compiled_message_state_at(
                                context,
                                compiled,
                                overlay_index,
                                write.beat,
                                seconds,
                            );
                            overlay_state_render_value(&state, write.target)
                        });
                let Some(actual) = actual else {
                    gaps.push(format!(
                        "layer {layer} missing {:?} state for {}/{} at beat {:.3}",
                        write.target, definition.id, definition.class, write.beat
                    ));
                    continue;
                };
                if !render_value_matches(&write.value, &actual) {
                    gaps.push(format!(
                        "layer {layer} {:?} differs for {}/{} at beat {:.3}: ITGmania {:?}, DeadSync {:?}",
                        write.target,
                        definition.id,
                        definition.class,
                        write.beat,
                        write.value,
                        actual
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
        Some("spring") => Some("spring"),
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
            "zoomz" => block.zoom_z = value_f32(operation.args.first()),
            "addrotationx" | "rotationx" => {
                block.rot_x = value_f32(operation.args.first());
            }
            "addrotationy" | "rotationy" => {
                block.rot_y = value_f32(operation.args.first());
            }
            "addrotationz" | "rotationz" => {
                block.rot_z = value_f32(operation.args.first());
            }
            "skewx" => block.skew_x = value_f32(operation.args.first()),
            "skewy" => block.skew_y = value_f32(operation.args.first()),
            "fov" => block.fov = value_f32(operation.args.first()),
            "vanishpoint" => {
                block.vanishpoint = Some([
                    value_f32(operation.args.first()).unwrap_or_default(),
                    value_f32(operation.args.get(1)).unwrap_or_default(),
                ]);
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
        || block.zoom_z.is_some()
        || block.rot_x.is_some()
        || block.rot_y.is_some()
        || block.rot_z.is_some()
        || block.skew_x.is_some()
        || block.skew_y.is_some()
        || block.fov.is_some()
        || block.vanishpoint.is_some()
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
        && option_f32_matches(expected.zoom_z, actual.delta.zoom_z)
        && option_f32_matches(expected.rot_x, actual.delta.rot_x_deg)
        && option_f32_matches(expected.rot_y, actual.delta.rot_y_deg)
        && option_f32_matches(expected.rot_z, actual.delta.rot_z_deg)
        && option_f32_matches(expected.skew_x, actual.delta.skew_x)
        && option_f32_matches(expected.skew_y, actual.delta.skew_y)
        && option_f32_matches(expected.fov, actual.delta.fov)
        && expected.vanishpoint.is_none_or(|expected| {
            actual.delta.vanishpoint.is_some_and(|actual| {
                (actual[0] - expected[0]).abs() <= EPSILON
                    && (actual[1] - expected[1]).abs() <= EPSILON
            })
        })
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

fn stateful_write_has_value(
    writes: &[SongLuaStatefulMessageWrite],
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    expected: &SongLuaOverlayUpdateValue,
    block: &ExpectedBlock,
) -> bool {
    writes.iter().any(|write| {
        write.overlay_index == overlay_index
            && write.target == target
            && (write.delay_seconds - block.start).abs() <= EPSILON
            && (write.duration_seconds - block.duration).abs() <= EPSILON
            && write.easing.as_deref() == block.easing
            && render_value_matches(expected, &write.value)
    })
}

fn stateful_block_matches(
    writes: &[SongLuaStatefulMessageWrite],
    overlay_index: usize,
    targets: &[SongLuaOverlayUpdateTarget],
    block: &ExpectedBlock,
) -> bool {
    use SongLuaOverlayUpdateTarget as Target;
    use SongLuaOverlayUpdateValue as UpdateValue;
    let has = |target, value| {
        targets.contains(&target)
            && stateful_write_has_value(writes, overlay_index, target, &value, block)
    };
    let f32_matches = |target, expected: Option<f32>| {
        expected.is_none_or(|value| has(target, UpdateValue::F32(value)))
    };
    block.alpha.is_none_or(|alpha| {
        targets.contains(&Target::Diffuse)
            && writes.iter().any(|write| {
                write.overlay_index == overlay_index
                    && write.target == Target::Diffuse
                    && (write.delay_seconds - block.start).abs() <= EPSILON
                    && (write.duration_seconds - block.duration).abs() <= EPSILON
                    && write.easing.as_deref() == block.easing
                    && matches!(&write.value, UpdateValue::Vec4(color) if (color[3] - alpha).abs() <= 0.03)
            })
    }) && block
        .visible
        .is_none_or(|value| has(Target::Visible, UpdateValue::Bool(value)))
        && f32_matches(Target::X, block.x)
        && f32_matches(Target::Y, block.y)
        && f32_matches(Target::Z, block.z)
        && f32_matches(Target::Zoom, block.zoom)
        && f32_matches(Target::ZoomX, block.zoom_x)
        && f32_matches(Target::ZoomY, block.zoom_y)
        && f32_matches(Target::ZoomZ, block.zoom_z)
        && f32_matches(Target::RotationX, block.rot_x)
        && f32_matches(Target::RotationY, block.rot_y)
        && f32_matches(Target::RotationZ, block.rot_z)
        && f32_matches(Target::SkewX, block.skew_x)
        && f32_matches(Target::SkewY, block.skew_y)
        && f32_matches(Target::Fov, block.fov)
        && block.vanishpoint.is_none_or(|value| {
            has(Target::Vanishpoint, UpdateValue::Vec2(value))
        })
        && f32_matches(Target::CropLeft, block.crop_left)
        && f32_matches(Target::CropRight, block.crop_right)
        && f32_matches(Target::CropTop, block.crop_top)
        && f32_matches(Target::CropBottom, block.crop_bottom)
        && block.sprite_state.is_none_or(|value| {
            has(Target::SpriteStateIndex, UpdateValue::U32(value))
        })
}

fn stateful_command_matches(
    writes: &[SongLuaStatefulMessageWrite],
    overlay_index: usize,
    targets: &[SongLuaOverlayUpdateTarget],
    expected: &ExpectedCommand,
) -> bool {
    expected
        .blocks
        .iter()
        .all(|(_, block)| stateful_block_matches(writes, overlay_index, targets, block))
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
    let mut missing = HashMap::<String, Vec<(String, NativeTarget, ExpectedCommand)>>::new();
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
            missing.entry(expected.message.clone()).or_default().push((
                label,
                expected.target.clone(),
                expected,
            ));
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
        let mut used_dynamic = HashSet::new();
        let mut unmatched = Vec::new();
        for (label, target, expected) in targets {
            let NativeTarget::Actor { layer, .. } = target else {
                unmatched.push(label);
                continue;
            };
            let Some(layer_compiled) = compiled.get(layer) else {
                unmatched.push(label);
                continue;
            };
            let candidate = layer_compiled
                .stateful_message_captures
                .iter()
                .enumerate()
                .filter(|(_, capture)| capture.message == message)
                .flat_map(|(capture_index, capture)| {
                    capture.overlay_targets.iter().enumerate().map(
                        move |(target_index, (overlay_index, properties))| {
                            (capture_index, target_index, *overlay_index, properties)
                        },
                    )
                })
                .find(|(capture_index, target_index, overlay_index, properties)| {
                    !used_dynamic.contains(&(layer, *capture_index, *target_index))
                        && stateful_command_matches(
                            &layer_compiled.stateful_message_captures[*capture_index].writes,
                            *overlay_index,
                            properties,
                            &expected,
                        )
                });
            if let Some((capture_index, target_index, _, _)) = candidate {
                used_dynamic.insert((layer, capture_index, target_index));
            } else {
                unmatched.push(label);
            }
        }
        if unmatched.is_empty() {
            continue;
        }
        let dynamic = compiled.iter().any(|compiled| {
            compiled
                .stateful_message_captures
                .iter()
                .any(|capture| capture.message == message)
                || compiled
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
            let captured = compiled
                .iter()
                .flat_map(|compiled| &compiled.stateful_message_captures)
                .filter(|capture| capture.message == message)
                .map(|capture| capture.overlay_targets.len())
                .sum::<usize>();
            gaps.push(format!(
                "stateful {message}MessageCommand differs: DeadSync captured {captured} actors, but {} ITGmania targets/properties did not match ({})",
                unmatched.len(),
                unmatched.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
            ));
        } else {
            gaps.extend(
                unmatched
                    .into_iter()
                    .map(|target| format!("missing {message}MessageCommand effects on {target}")),
            );
        }
    }
}

fn native_player_operation_target(operation: &str) -> Option<(SongLuaEaseTarget, f32)> {
    let method = operation.rsplit('.').next()?.to_ascii_lowercase();
    Some(match method.as_str() {
        "x" => (SongLuaEaseTarget::PlayerX, 0.0),
        "y" => (SongLuaEaseTarget::PlayerY, 0.0),
        "z" => (SongLuaEaseTarget::PlayerZ, 0.0),
        "rotationx" => (SongLuaEaseTarget::PlayerRotationX, 0.0),
        "rotationy" => (SongLuaEaseTarget::PlayerRotationY, 0.0),
        "rotationz" => (SongLuaEaseTarget::PlayerRotationZ, 0.0),
        "skewx" => (SongLuaEaseTarget::PlayerSkewX, 0.0),
        "skewy" => (SongLuaEaseTarget::PlayerSkewY, 0.0),
        "zoom" => (SongLuaEaseTarget::PlayerZoom, 1.0),
        "zoomx" => (SongLuaEaseTarget::PlayerZoomX, 1.0),
        "zoomy" => (SongLuaEaseTarget::PlayerZoomY, 1.0),
        "zoomz" => (SongLuaEaseTarget::PlayerZoomZ, 1.0),
        _ => return None,
    })
}

fn compiled_player_range(
    compiled: &[CompiledSongLua],
    player: u8,
    target: &SongLuaEaseTarget,
    default: f32,
) -> (f32, f32) {
    compiled
        .iter()
        .flat_map(|layer| &layer.eases)
        .filter(|ease| {
            ease.target == *target && (ease.player.is_none() || ease.player == Some(player))
        })
        .flat_map(|ease| [ease.from, ease.to])
        .fold((default, default), |(min, max), value| {
            (min.min(value), max.max(value))
        })
}

fn range_covers(actual: (f32, f32), expected: (f32, f32)) -> bool {
    actual.0 <= expected.0 + 0.03 && actual.1 >= expected.1 - 0.03
}

fn compare_player_operation_ranges(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    gaps: &mut Vec<String>,
) {
    for track in &trace.operation_tracks {
        let Some(actor) = trace
            .external_actors
            .iter()
            .find(|actor| actor.id == track.actor)
        else {
            continue;
        };
        let player = if actor.path.ends_with("PlayerP1") {
            1
        } else if actor.path.ends_with("PlayerP2") {
            2
        } else {
            continue;
        };
        let Some((target, default)) = native_player_operation_target(&track.operation) else {
            continue;
        };
        let values = track
            .samples
            .iter()
            .filter_map(|sample| sample.3.first().and_then(|value| value_f32(Some(value))))
            .collect::<Vec<_>>();
        let Some(native_min) = values.iter().copied().reduce(f32::min) else {
            continue;
        };
        let native_max = values
            .iter()
            .copied()
            .reduce(f32::max)
            .unwrap_or(native_min);
        if (native_min - default).abs() <= EPSILON && (native_max - default).abs() <= EPSILON {
            continue;
        }
        let expected = (native_min, native_max);
        let compiled_range = compiled_player_range(compiled, player, &target, default);
        let axis_ranges = (target == SongLuaEaseTarget::PlayerZoom).then(|| {
            [
                SongLuaEaseTarget::PlayerZoomX,
                SongLuaEaseTarget::PlayerZoomY,
                SongLuaEaseTarget::PlayerZoomZ,
            ]
            .map(|axis| compiled_player_range(compiled, player, &axis, default))
        });
        let covered = range_covers(compiled_range, expected)
            || axis_ranges.is_some_and(|ranges| {
                ranges
                    .into_iter()
                    .all(|range| range_covers(range, expected))
            });
        if !covered {
            gaps.push(format!(
                "P{player} {} range differs: ITGmania [{native_min:.3}, {native_max:.3}], DeadSync [{:.3}, {:.3}]",
                track.operation,
                compiled_range.0,
                compiled_range.1
            ));
        }
    }
}

fn compiled_message_state_at(
    context: &SongLuaCompileContext,
    compiled: &CompiledSongLua,
    overlay_index: usize,
    beat: f32,
    seconds: f32,
) -> SongLuaOverlayState {
    let overlay = &compiled.overlays[overlay_index];
    let mut current = overlay.initial_state;
    let mut active = None::<(&[SongLuaOverlayCommandBlock], SongLuaOverlayState, f32)>;
    for event in compiled.messages.iter().filter(|event| event.beat <= beat) {
        let Some(command) = overlay
            .message_commands
            .iter()
            .find(|command| command.message == event.message)
        else {
            continue;
        };
        let event_seconds = song_elapsed_seconds_at(event.beat, context);
        if let Some((blocks, base, start_seconds)) = active.take() {
            current = overlay_state_after_blocks(base, blocks, event_seconds - start_seconds);
        }
        let base = current;
        current = overlay_state_after_blocks(base, &command.blocks, 0.0);
        active = Some((&command.blocks, base, event_seconds));
    }
    if let Some((blocks, base, start_seconds)) = active {
        current = overlay_state_after_blocks(base, blocks, seconds - start_seconds);
    }
    current
}

fn compare_projected_vibration_coverage(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    gaps: &mut Vec<String>,
) {
    let definitions = trace
        .actor_definitions
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect::<HashMap<_, _>>();
    let mut drawable_map = HashMap::<&str, (usize, usize)>::new();
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
        if native.len() == deadsync.len() {
            drawable_map.extend(
                native
                    .into_iter()
                    .zip(deadsync)
                    .map(|(definition, index)| (definition.id.as_str(), (layer, index))),
            );
        }
    }
    for track in &trace.projected_vertex_tracks {
        let Some(definition_id) = track.definition_id.as_deref() else {
            continue;
        };
        let Some(&(layer, overlay_index)) = drawable_map.get(definition_id) else {
            continue;
        };
        for sample in &track.samples {
            let Some(sample) = sample.as_array() else {
                continue;
            };
            let Some(beat) = sample.first().and_then(|value| value_f32(Some(value))) else {
                continue;
            };
            let Some(seconds) = sample.get(1).and_then(|value| value_f32(Some(value))) else {
                continue;
            };
            let native = sample
                .get(8)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|effect| effect.get("mode").and_then(Value::as_str) == Some("vibrate"))
                .filter_map(|effect| effect.get("magnitude").and_then(Value::as_array))
                .fold([0.0_f32; 3], |mut sum, magnitude| {
                    for (axis, value) in magnitude.iter().take(3).enumerate() {
                        sum[axis] += value_f32(Some(value)).unwrap_or_default();
                    }
                    sum
                });
            let mut actual = [0.0_f32; 3];
            let mut current = Some(overlay_index);
            while let Some(index) = current {
                let overlay = &compiled[layer].overlays[index];
                let message_state =
                    compiled_message_state_at(context, &compiled[layer], index, beat, seconds);
                let vibrate = compiled_update_value_at(
                    &compiled[layer],
                    index,
                    SongLuaOverlayUpdateTarget::Vibrate,
                    beat,
                )
                .and_then(|value| match value {
                    SongLuaOverlayUpdateValue::Bool(value) => Some(value),
                    _ => None,
                })
                .unwrap_or(message_state.vibrate);
                if vibrate {
                    let magnitude = compiled_update_value_at(
                        &compiled[layer],
                        index,
                        SongLuaOverlayUpdateTarget::EffectMagnitude,
                        beat,
                    )
                    .and_then(|value| match value {
                        SongLuaOverlayUpdateValue::Vec3(value) => Some(value),
                        _ => None,
                    })
                    .unwrap_or(message_state.effect_magnitude);
                    for axis in 0..3 {
                        actual[axis] += magnitude[axis];
                    }
                }
                current = overlay.parent_index;
            }
            if native
                .iter()
                .zip(actual)
                .any(|(expected, actual)| (expected - actual).abs() > 0.03)
            {
                gaps.push(format!(
                    "projected vibration differs for {definition_id} at beat {beat:.3}: ITGmania {native:?}, DeadSync {actual:?}"
                ));
            }
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
    let (compiled, primary_index, context) = compile_trace_song(&trace);
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
    compare_update_render_values(&trace, &compiled, &context, &mut gaps);
    compare_player_operation_ranges(&trace, &compiled, &mut gaps);
    compare_projected_vibration_coverage(&trace, &compiled, &context, &mut gaps);
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
#[ignore = "compiles the complete Cuphead song-Lua runtime at 60 Hz"]
fn cuphead_stateful_fire_message_matches_itgmania() {
    let trace = read_trace_file(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CUPHEAD_TRACE));
    let (compiled, primary_index, _) = compile_trace_song(&trace);
    for message in ["CagneyInit", "TargetsOn", "GoatSlap"] {
        assert!(
            compiled
                .iter()
                .flat_map(|layer| &layer.messages)
                .any(|event| event.message == message),
            "Cuphead runtime action table lost the {message} broadcast"
        );
    }
    let fire = compiled
        .iter()
        .flat_map(|layer| &layer.stateful_message_captures)
        .filter(|capture| capture.message == "Fire")
        .collect::<Vec<_>>();
    let affected = fire
        .iter()
        .map(|capture| capture.overlay_targets.len())
        .sum::<usize>();
    assert_eq!(affected, 28, "Cuphead Fire must retain all pooled targets");

    let mut gaps = Vec::new();
    compare_commands(&trace, &compiled, primary_index, &mut gaps);
    assert!(
        gaps.is_empty(),
        "Cuphead message-command parity gaps ({}):\n- {}",
        gaps.len(),
        gaps.join("\n- ")
    );

    let native_beat = cuphead_flower_spawn_beat(&trace);
    let (layer, overlay_index) = trace
        .roots
        .iter()
        .zip(&compiled)
        .enumerate()
        .find_map(|(layer, (_, compiled))| {
            compiled
                .overlays
                .iter()
                .find_map(|overlay| {
                    let SongLuaOverlayKind::Sprite { texture_path, .. } = &overlay.kind else {
                        return None;
                    };
                    let texture = texture_path.to_string_lossy().replace('\\', "/");
                    if !texture.contains("/cagney/sprout") {
                        return None;
                    }
                    let parent_index = overlay.parent_index?;
                    let parent = &compiled.overlays[parent_index];
                    (matches!(&parent.kind, SongLuaOverlayKind::ActorFrame)
                        && !parent.initial_state.visible)
                        .then_some(parent_index)
                })
                .map(|overlay_index| (layer, overlay_index))
        })
        .expect("DeadSync does not contain the Cuphead flower actor");
    let actual_beat = compiled[layer]
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == overlay_index
                && track.target == SongLuaOverlayUpdateTarget::Visible
        })
        .and_then(|track| {
            track.samples.iter().find_map(|sample| {
                (matches!(&sample.value, SongLuaOverlayUpdateValue::Bool(true))
                    && (299.0..=301.0).contains(&sample.beat))
                .then_some(sample.beat)
            })
        })
        .expect("DeadSync never makes the Cuphead flower actor visible");
    assert!(
        (actual_beat - native_beat).abs() <= EPSILON,
        "Cuphead flower spawn differs: ITGmania beat {native_beat}, DeadSync beat {actual_beat}"
    );

    let native_beat = cuphead_cagney_spawn_beat(&trace);
    let (layer, _cagney_index) = compiled
        .iter()
        .enumerate()
        .find_map(|(layer_index, layer)| {
            layer
                .overlays
                .iter()
                .position(|actor| {
                    let SongLuaOverlayKind::Sprite { texture_path, .. } = &actor.kind else {
                        return false;
                    };
                    texture_path
                        .to_string_lossy()
                        .replace('\\', "/")
                        .contains("/cagney/idle")
                        && !actor.initial_state.visible
                        && actor.message_commands.iter().any(|command| {
                            command.message == "CagneyInit"
                                && command
                                    .blocks
                                    .iter()
                                    .any(|block| block.delta.visible == Some(true))
                        })
                })
                .map(|actor_index| (layer_index, actor_index))
        })
        .expect("DeadSync does not contain the Cuphead Cagney boss actor");
    let actual_beat = compiled[layer]
        .messages
        .iter()
        .find(|event| event.message == "CagneyInit" && (247.0..=249.0).contains(&event.beat))
        .map(|event| event.beat)
        .expect("DeadSync never starts the Cuphead Cagney phase");
    assert!(
        (actual_beat - native_beat).abs() <= EPSILON,
        "Cuphead Cagney spawn differs: ITGmania beat {native_beat}, DeadSync beat {actual_beat}"
    );
}

fn cuphead_flower_spawn_beat(trace: &NativeTrace) -> f32 {
    trace
        .tween_tracks
        .iter()
        .filter(|track| {
            track.actor == "def-0137" && track.command.as_deref() == Some("UpdateCommand")
        })
        .flat_map(|track| &track.segments)
        .find(|segment| {
            (299.0..=301.0).contains(&segment.beat)
                && segment.operations.iter().any(|operation| {
                    operation.operation == "ActorFrame.visible"
                        && operation.args.first().and_then(Value::as_bool) == Some(true)
                })
        })
        .map(|segment| segment.beat)
        .expect("Cuphead fixture does not capture the flower spawn")
}

fn cuphead_cagney_spawn_beat(trace: &NativeTrace) -> f32 {
    trace
        .tween_tracks
        .iter()
        .filter(|track| {
            track.actor == "def-0030"
                && track.command.as_deref() == Some("CagneyInitMessageCommand")
        })
        .flat_map(|track| &track.segments)
        .find(|segment| {
            (247.0..=249.0).contains(&segment.beat)
                && segment.operations.iter().any(|operation| {
                    operation.operation == "Sprite.visible"
                        && operation.args.first().and_then(Value::as_bool) == Some(true)
                })
        })
        .map(|segment| segment.beat)
        .expect("Cuphead fixture does not capture the Cagney boss spawn")
}

fn cuphead_cagney_parent_segments(
    trace: &NativeTrace,
) -> impl Iterator<Item = (&NativeTweenTrack, &NativeTweenSegment)> {
    trace
        .tween_tracks
        .iter()
        .filter(|track| {
            track.actor == "def-0029" && track.command.as_deref() == Some("UpdateCommand")
        })
        .flat_map(|track| track.segments.iter().map(move |segment| (track, segment)))
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
    assert_eq!(manifest.simfiles.len(), 43);
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
        let fixture_path = fixture_root.join(&entry.fixture);
        let fixture: Value =
            serde_json::from_slice(&fs::read(&fixture_path).unwrap_or_else(|error| {
                panic!("missing fixture {}: {error}", fixture_path.display())
            }))
            .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", fixture_path.display()));
        assert_eq!(fixture["capabilities"]["render_state_calls"], true);
        assert_eq!(
            fixture["capabilities"]["source_derived_transform_model"],
            true
        );
        assert_eq!(fixture["capabilities"]["external_actor_paths"], true);
        assert_eq!(fixture["capabilities"]["player_render_samples"], true);
        assert_eq!(fixture["capabilities"]["raster_output"], false);
        assert_eq!(
            fixture["semantic_derivation"]["render_model"]["projection_width_basis"],
            "SCREEN_WIDTH"
        );
    }
}

#[test]
fn cuphead_fixture_captures_queued_boss_spawns() {
    let trace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CUPHEAD_TRACE);
    let trace = read_trace_file(&trace_path);

    let flower_beat = cuphead_flower_spawn_beat(&trace);
    assert!(
        flower_beat > 300.0 && flower_beat < 300.1,
        "queued Cuphead flower spawn ran at unexpected beat {flower_beat}"
    );
    let cagney_beat = cuphead_cagney_spawn_beat(&trace);
    assert!(
        cagney_beat > 248.0 && cagney_beat < 248.1,
        "queued Cuphead Cagney spawn ran at unexpected beat {cagney_beat}"
    );

    let exit = cuphead_cagney_parent_segments(&trace)
        .find(|(track, segment)| {
            track.kind == "tween"
                && track.easing.as_deref() == Some("accelerate")
                && (111.0..111.1).contains(&segment.beat)
        })
        .expect("Cuphead fixture lost Cagney's beat-111 exit tween");
    assert!(exit.1.operations.iter().any(|operation| {
        operation.operation == "ActorFrame.addx" && value_f32(operation.args.first()) == Some(427.0)
    }));
    assert!(
        exit.1
            .operations
            .iter()
            .all(|operation| operation.operation != "ActorFrame.zoom"),
        "the beat-248 return was folded into Cagney's completed exit tween"
    );
    let reentry = cuphead_cagney_parent_segments(&trace)
        .find(|(track, segment)| {
            track.kind == "immediate"
                && (248.0..248.1).contains(&segment.beat)
                && segment.operations.iter().any(|operation| {
                    operation.operation == "ActorFrame.x"
                        && value_f32(operation.args.first()) == Some(427.0)
                })
        })
        .expect("Cuphead fixture did not separate Cagney's beat-248 return");
    assert!(reentry.1.beat - exit.1.beat > 136.0);
}

#[test]
fn cuphead_fixture_captures_impact_rotation_and_cannon_vibration() {
    let trace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CUPHEAD_TRACE);
    let trace = read_trace_file(&trace_path);

    let player_rotations = trace
        .operation_tracks
        .iter()
        .filter(|track| {
            track.operation.eq_ignore_ascii_case("ActorFrame.rotationx")
                && trace.external_actors.iter().any(|actor| {
                    actor.id == track.actor
                        && matches!(
                            actor.path.as_str(),
                            "ScreenGameplay/PlayerP1" | "ScreenGameplay/PlayerP2"
                        )
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(player_rotations.len(), 2);
    for track in player_rotations {
        assert!(
            track.samples.iter().any(|sample| {
                value_f32(sample.3.first()).is_some_and(|value| (value - 90.0).abs() <= EPSILON)
            }),
            "{} never reaches the authored 90-degree impact rotation",
            track.actor
        );
        assert!(
            track.samples.iter().any(|sample| {
                value_f32(sample.3.first()).is_some_and(|value| value.abs() <= EPSILON)
            }),
            "{} never restores its impact rotation",
            track.actor
        );
    }

    let cannon_girl = trace
        .projected_vertex_tracks
        .iter()
        .find(|track| track.texture.ends_with("ayaze/idle 2x2.png"))
        .expect("Cuphead fixture has no cannongirl geometry");
    assert_eq!(
        cannon_girl.sample_layout.last().map(String::as_str),
        Some("effect_chain")
    );
    assert!(
        cannon_girl
            .samples
            .iter()
            .filter_map(Value::as_array)
            .any(|sample| {
                sample
                    .get(8)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|effect| effect.get("mode").and_then(Value::as_str) == Some("vibrate"))
                    .filter_map(|effect| effect.get("magnitude").and_then(Value::as_array))
                    .flatten()
                    .filter_map(|value| value.as_f64())
                    .any(|value| value.abs() > f64::from(EPSILON))
            }),
        "Cuphead fixture never records the cannongirl's inherited vibration"
    );
}

#[test]
fn step_your_game_up_critical_render_states_match_itgmania() {
    let trace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(STEP_YOUR_GAME_UP_TRACE);
    let trace = read_trace_file(&trace_path);
    let (compiled, _, context) = compile_trace_song(&trace);
    let mut gaps = Vec::new();
    compare_update_render_values(&trace, &compiled, &context, &mut gaps);
    let critical = gaps
        .into_iter()
        .filter(|gap| {
            gap.contains("beat 53.500")
                || gap.contains("beat 68.000")
                || gap.contains("beat 70.000")
                || gap.contains("beat 72.000")
                || gap.contains("beat 200.000")
        })
        .collect::<Vec<_>>();
    assert!(
        critical.is_empty(),
        "Step Your Game Up critical render parity gaps ({}):\n- {}",
        critical.len(),
        critical.join("\n- ")
    );
    assert_step_player_proxy_and_projection(&trace, &compiled);
}

fn assert_step_player_proxy_and_projection(trace: &NativeTrace, compiled: &[CompiledSongLua]) {
    for player in 1..=2 {
        let path = format!("ScreenGameplay/PlayerP{player}");
        let external = trace
            .external_actors
            .iter()
            .find(|actor| actor.path == path)
            .unwrap_or_else(|| panic!("ITGmania trace is missing {path}"));
        assert!(external.id.starts_with("external-"));
        assert_eq!(external.class, "ActorFrame");

        let track = trace
            .player_render_tracks
            .iter()
            .find(|track| track.player == player)
            .unwrap_or_else(|| panic!("ITGmania trace is missing PlayerP{player} render state"));
        assert_eq!(track.path, path);
        let hidden = track
            .samples
            .iter()
            .find(|sample| sample.0 >= 68.0 && sample.0 <= 68.1)
            .expect("missing Player proxy-off boundary at beat 68");
        let restored = track
            .samples
            .iter()
            .find(|sample| sample.0 >= 70.0 && sample.0 <= 70.1)
            .expect("missing Player proxy-on boundary at beat 70");
        assert_eq!((hidden.2, hidden.3, hidden.4), (false, false, false));
        assert_eq!((restored.2, restored.3, restored.4), (false, true, true));

        let proxy_id = restored
            .5
            .first()
            .expect("restored Player has no visible ActorProxy");
        let definitions = trace
            .actor_definitions
            .iter()
            .map(|definition| (definition.id.as_str(), definition))
            .collect::<HashMap<_, _>>();
        let (layer, overlay_index) = trace
            .roots
            .iter()
            .zip(compiled)
            .find_map(|(root_id, layer)| {
                let root = definitions.get(root_id.as_str())?;
                let mut native = Vec::new();
                collect_native_overlay_definitions(root, &definitions, &mut native);
                native
                    .iter()
                    .position(|definition| definition.id == *proxy_id)
                    .map(|index| (layer, index))
            })
            .unwrap_or_else(|| panic!("visible proxy {proxy_id} is not in native draw topology"));
        let SongLuaOverlayKind::ActorProxy { target } = layer.overlays[overlay_index].kind.clone()
        else {
            panic!("native Player proxy is not a DeadSync ActorProxy");
        };
        assert_eq!(
            target,
            deadsync_assets::song_lua::SongLuaProxyTarget::Player {
                player_index: player - 1
            }
        );
        assert_eq!(
            compiled_update_value_at(
                layer,
                overlay_index,
                SongLuaOverlayUpdateTarget::Visible,
                hidden.0 as f32,
            ),
            Some(SongLuaOverlayUpdateValue::Bool(false))
        );
        assert_eq!(
            compiled_update_value_at(
                layer,
                overlay_index,
                SongLuaOverlayUpdateTarget::Visible,
                restored.0 as f32,
            ),
            Some(SongLuaOverlayUpdateValue::Bool(true))
        );
    }

    let circle = trace
        .projected_vertex_tracks
        .iter()
        .find(|track| track.texture.ends_with("tpe3 circ 2.png"))
        .expect("missing projected circle geometry");
    assert_eq!(circle.definition_id.as_deref(), Some(circle.actor.as_str()));
    assert_eq!(circle.texture_size, [710.0, 710.0]);
    assert!(!circle.camera_actor.is_empty());
    assert_eq!(
        circle.sample_layout,
        [
            "beat",
            "seconds",
            "visible",
            "alpha",
            "world_vertices",
            "clip_vertices",
            "screen_vertices",
            "camera",
            "effect_chain",
        ]
    );
    let sample = circle
        .samples
        .iter()
        .filter_map(Value::as_array)
        .find(|sample| {
            value_f32(sample.first()).is_some_and(|beat| (200.0..=200.1).contains(&beat))
        })
        .expect("missing projected circle sample at beat 200");
    let clip_vertices = sample
        .get(5)
        .and_then(Value::as_array)
        .expect("projected sample has no homogeneous clip vertices");
    assert_eq!(clip_vertices.len(), 4);
    assert!(
        clip_vertices.iter().any(|vertex| {
            vertex
                .as_array()
                .and_then(|values| value_f32(values.get(3)))
                .is_some_and(|w| w <= 0.0)
        }),
        "circle fixture must exercise ITGmania near-plane clipping"
    );
}

#[test]
fn starred_modifier_normalization_ignores_existing_options() {
    assert_eq!(
        starred_mods("Overhead, 100% Dark, *1 no dark, *2 80% stealth"),
        "*1 no dark, *2 80% stealth"
    );
}
