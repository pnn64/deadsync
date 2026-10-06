#[path = "support/paths.rs"]
mod paths;

use deadlib_present::anim::EffectMode;
use deadsync_assets::song_lua::{
    CompiledSongLua, SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget,
    SongLuaOverlayCommandBlock, SongLuaOverlayKind, SongLuaOverlayState, SongLuaOverlayStateDelta,
    SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue, SongLuaPlayerContext, SongLuaSpanMode,
    SongLuaSpeedMod, SongLuaStatefulMessageWrite, SongLuaTimeUnit, compile_song_lua_layers,
    overlay_state_after_blocks, parse_song_timing_bpms, song_elapsed_seconds_at,
};
use deadsync_simfile::song::{ParseSongOptions, parse_song_meta_file};
use deadsync_song_lua::playback::actor_conformance::compose_overlay_states;
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
const BROGAMER_TRACE: &str = "tests/fixtures/itgmania-song-lua/BroGamer/BroGamer.ssc.semantic.json";
const COSMIC_TRACE: &str = "tests/fixtures/itgmania-song-lua/[11] CO5M1C R4ILR0AD (SH) [TaroNuke vs. Scrypts]/CO5M1C R4ILR0AD-chart.ssc.semantic.json";
const RIDDLE_DOUBLE_TRACE: &str = "tests/fixtures/itgmania-song-lua/[10] Riddle (DX) [Brother Mojo remixes A. Astral]/Riddle.ssc.semantic.json";
const FLIP69_TRACE: &str =
    "tests/fixtures/itgmania-song-lua/[10] flip69 (DX) [Telperion]/flip69.ssc.semantic.json";
const SEMANTIC_MANIFEST: &str = "_semantic_manifest.json";
const EPSILON: f32 = 0.002;
const PROJECTED_BEAT_EPSILON: f32 = 0.005;

#[path = "song_lua_itgmania_semantic_parity/whole_song_archives.rs"]
mod whole_song_archives;

#[path = "song_lua_itgmania_semantic_parity/runtime_modifiers.rs"]
mod runtime_modifiers;

#[path = "song_lua_itgmania_semantic_parity/multitap.rs"]
mod multitap;

#[path = "song_lua_itgmania_semantic_parity/corpora.rs"]
mod corpora;

#[derive(Deserialize)]
struct NativeTrace {
    #[serde(default)]
    arrow_timing: String,
    #[serde(default)]
    random_seed: Option<u32>,
    #[serde(default)]
    difficulty: String,
    #[serde(default)]
    steps_type: String,
    #[serde(default)]
    description: String,
    oracle: String,
    title: String,
    style: String,
    #[serde(default)]
    enabled_players: Option<[bool; 2]>,
    #[serde(default)]
    noteskin_reference: Option<NativeNoteskin>,
    simfile: String,
    source_simfile: Option<PathBuf>,
    roots: Vec<String>,
    actor_definitions: Vec<NativeDefinition>,
    runtime_actors: Vec<NativeActor>,
    timeline_tracks: Vec<NativeTimelineTrack>,
    #[serde(default)]
    command_tracks: Vec<NativeCommandTrack>,
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
    #[serde(default)]
    manual_draw_frames: Vec<(f64, f64, Vec<Value>)>,
    #[serde(default)]
    update_frames: Vec<(f64, f64)>,
    end_position: NativePosition,
    display: NativeDisplay,
    fixture_context: NativeFixtureContext,
    trace_until_beat: f32,
}

#[derive(Deserialize)]
struct NativeNoteskin {
    skin: String,
    #[serde(default)]
    files: Vec<NativeNoteskinFile>,
}

#[derive(Deserialize)]
struct NativeNoteskinFile {
    path: PathBuf,
    sha256: String,
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
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    final_render_state: Option<NativeRenderSnapshot>,
    #[serde(default)]
    render_state_samples: Vec<(usize, Option<f32>, bool)>,
}

#[derive(Deserialize)]
struct NativeRenderSnapshot {
    alpha: Option<f32>,
    visible: bool,
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
    #[serde(default)]
    transform_samples: Vec<[Option<f32>; 12]>,
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
    #[serde(default)]
    texture_samples: Vec<(f32, f32, String, f32, f32)>,
}

#[derive(Deserialize)]
struct NativeDrawOrder {
    parent_definition_id: String,
    #[serde(default)]
    parent_actor: String,
    instance: usize,
    final_children: Vec<NativeDrawChild>,
}

#[derive(Deserialize)]
struct NativeDrawChild {
    definition_id: String,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    layer_index: usize,
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
struct NativeCommandTrack {
    command: String,
    runs: Vec<(u64, Option<f32>, Option<f32>)>,
}

#[derive(Deserialize)]
struct NativeTweenSegment {
    enqueue_seq: u64,
    beat: f32,
    #[serde(default)]
    seconds: Option<f32>,
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
    /// Empty for calls on non-actor objects such as `SoundManager`.
    #[serde(default)]
    actor: String,
    operation: String,
    samples: Vec<(u64, f32, f32, Vec<Value>)>,
}

#[derive(Deserialize)]
struct NativePosition {
    #[serde(default)]
    beat: Option<f32>,
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
    #[serde(default)]
    note_end_beat: Option<f32>,
    #[serde(default)]
    specified_last_beat: Option<f32>,
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
    simfile: String,
    fixture: PathBuf,
    status: String,
    #[serde(default)]
    runtime_errors: usize,
    #[serde(default)]
    dropped_events: usize,
}

/// Every native-vs-DeadSync comparison, grouped by the comparator that made
/// it, so a report states how many checks passed rather than only the gaps.
#[derive(Default)]
struct Parity {
    sections: Vec<ParitySection>,
    gaps: Vec<String>,
    stage_started: Option<std::time::Instant>,
}

struct ParitySection {
    name: &'static str,
    checks: usize,
    failed: usize,
}

impl Parity {
    fn section(&mut self, name: &'static str) {
        self.log_stage();
        if std::env::var_os("DEADSYNC_SONG_LUA_TIMING_STDERR").is_some() {
            self.stage_started = Some(std::time::Instant::now());
        }
        self.sections.push(ParitySection {
            name,
            checks: 0,
            failed: 0,
        });
    }

    fn log_stage(&self) {
        if let (Some(start), Some(section)) = (self.stage_started, self.sections.last()) {
            eprintln!(
                "Song lua semantic timing: {} elapsed_ms={:.3}",
                section.name,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    fn check(&mut self, ok: bool, gap: impl FnOnce() -> String) {
        self.check_once(ok, &mut false, gap);
    }

    /// Counts every comparison but describes only the first failure behind
    /// `reported`, so dense per-frame samples do not flood the gap list.
    fn check_once(&mut self, ok: bool, reported: &mut bool, gap: impl FnOnce() -> String) {
        self.tally(1, usize::from(!ok));
        if !ok && !std::mem::replace(reported, true) {
            self.gaps.push(gap());
        }
    }

    /// Records `checks` comparisons, `failed` of them mismatching, for
    /// comparators that describe their failures in aggregated gap lines.
    fn tally(&mut self, checks: usize, failed: usize) {
        let section = self
            .sections
            .last_mut()
            .expect("comparators open a parity section before checking");
        section.checks += checks;
        section.failed += failed;
    }

    fn checks(&self) -> usize {
        self.sections.iter().map(|section| section.checks).sum()
    }

    fn passed(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.checks - section.failed)
            .sum()
    }

    fn summary(&self, title: &str) -> String {
        self.log_stage();
        let mut out = format!("{title}: {}", parity_status(self.passed(), self.checks()));
        let sections = self.sections.iter().filter(|section| section.checks > 0);
        let width = sections.clone().map(|section| section.name.len()).max();
        for section in sections {
            out.push_str(&format!(
                "\n  {:width$}  {}",
                section.name,
                parity_status(section.checks - section.failed, section.checks),
                width = width.unwrap_or_default()
            ));
        }
        out
    }

    fn assert_complete(&self, title: &str) {
        assert!(
            self.gaps.is_empty(),
            "{title} parity gaps ({}):\n- {}",
            self.gaps.len(),
            self.gaps.join("\n- ")
        );
    }
}

fn parity_status(passed: usize, checks: usize) -> String {
    let verdict = if passed == checks { "ok" } else { "FAILED" };
    format!("{passed}/{checks} {verdict}")
}

fn workspace_root() -> PathBuf {
    if let Some(path) = std::env::var_os("ITGMANIA_SONG_LUA_WORKSPACE") {
        return PathBuf::from(path);
    }
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
    if path.extension().is_some_and(|extension| extension == "zst") {
        let input = fs::File::open(path).unwrap_or_else(|error| {
            panic!("failed to open native trace {}: {error}", path.display())
        });
        let decoder = zstd::stream::read::Decoder::new(input).unwrap_or_else(|error| {
            panic!("failed to decode native trace {}: {error}", path.display())
        });
        return serde_json::from_reader(std::io::BufReader::new(decoder))
            .unwrap_or_else(|error| panic!("invalid native trace {}: {error}", path.display()));
    }
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
    context.background_layer_count = song.background_lua_changes.len();
    if let Some(seed) = trace.random_seed {
        context.random_seed = seed;
    }
    if trace.arrow_timing == "native" {
        let candidates = song.charts.iter().enumerate().filter(|(_, chart)| {
            chart.chart_type == trace.steps_type
                && chart
                    .difficulty
                    .eq_ignore_ascii_case(trace.difficulty.trim_start_matches("Difficulty_"))
        });
        let chart_index = candidates
            .clone()
            .find(|(_, chart)| chart.description == trace.description)
            .or_else(|| {
                // DeadSync trims Unicode whitespace from descriptions; native
                // ITGmania retains a trailing NBSP in Sharkmode. Keep exact
                // matches first and never choose an ambiguous trimmed match.
                let mut trimmed = candidates
                    .filter(|(_, chart)| chart.description.trim() == trace.description.trim());
                let first = trimmed.next()?;
                trimmed.next().is_none().then_some(first)
            })
            .map(|(index, _)| index)
            .unwrap_or_else(|| {
                panic!(
                    "DeadSync has no chart matching ITGmania's {} {} {:?}; its charts are {:?}",
                    trace.steps_type,
                    trace.difficulty,
                    trace.description,
                    song.charts
                        .iter()
                        .map(|chart| (&chart.chart_type, &chart.difficulty, &chart.description))
                        .collect::<Vec<_>>()
                )
            });
        let payload = deadsync_simfile::cache::load_gameplay_charts_with_options(
            &song,
            &[chart_index],
            &deadsync_simfile::cache::GameplayChartLoadOptions {
                cache_dir: Path::new("."),
                parse_options: &ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
                allow_cache_read: false,
                allow_cache_write: false,
                verify_cache_freshness: true,
                global_offset_seconds: 0.0,
            },
            |_| 0.0,
        )
        .expect("load reference chart timing");
        context.player_timing = std::array::from_fn(|_| Some(payload.charts[0].timing.clone()));
    }
    let timing_bpms = parse_song_timing_bpms(&song.normalized_bpms);
    context.song_timing = song.song_timing.clone();
    if !timing_bpms.is_empty() {
        context.song_timing_bpms = timing_bpms;
    }
    context.music_length_seconds = trace.end_position.seconds;
    context.style_name = trace.style.clone();
    context.screen_width = trace.display.logical_width;
    context.screen_height = trace.display.logical_height;
    context.display_width = trace.display.width;
    context.display_height = trace.display.height;
    // Match gameplay's AFT capability advertisement so renderer-gated charts
    // exercise their texture effects during semantic comparisons as well.
    context.video_renderers = "opengl,software".to_string();
    let player_x = [
        ((0.85 / 3.0) * context.screen_width).floor(),
        ((2.15 / 3.0) * context.screen_width).floor(),
    ];
    let player_x = if trace.style == "double" {
        [context.screen_width * 0.5; 2]
    } else {
        player_x
    };
    let enabled_players = trace.enabled_players.unwrap_or([true; 2]);
    // The oracle ran the chart it recorded; older fixtures record none.
    let difficulty = [
        SongLuaDifficulty::Beginner,
        SongLuaDifficulty::Easy,
        SongLuaDifficulty::Medium,
        SongLuaDifficulty::Hard,
        SongLuaDifficulty::Challenge,
        SongLuaDifficulty::Edit,
    ]
    .into_iter()
    .find(|difficulty| difficulty.sm_name() == trace.difficulty)
    .unwrap_or(SongLuaDifficulty::Challenge);
    context.players = [
        SongLuaPlayerContext {
            enabled: enabled_players[0],
            difficulty,
            speedmod: SongLuaSpeedMod::X(1.0),
            screen_x: player_x[0],
            screen_y: context.screen_height * 0.5,
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: enabled_players[1],
            difficulty,
            speedmod: SongLuaSpeedMod::X(1.0),
            screen_x: player_x[1],
            screen_y: context.screen_height * 0.5,
            ..SongLuaPlayerContext::default()
        },
    ];

    if let Some(noteskin) = &trace.noteskin_reference {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/noteskins");
        for file in &noteskin.files {
            assert!(
                file.path
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
            );
            assert_eq!(
                whole_song_archives::hash_file(&root.join(&file.path)),
                file.sha256,
                "noteskin dependency changed: {}",
                file.path.display()
            );
        }
        for player in &mut context.players {
            player.noteskin_name = noteskin.skin.clone();
        }
    }

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
        SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::WrapperState => "ActorFrame",
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
        // A compiled noteskin model uses cached slots for rendering.
        SongLuaOverlayKind::NoteskinActor { slots }
            if !slots.is_empty() && slots.iter().all(|slot| slot.model.is_some()) =>
        {
            "Model"
        }
        SongLuaOverlayKind::NoteskinActor { .. } => "NoteskinActor",
        SongLuaOverlayKind::SongMeterDisplay { .. } => "SongMeterDisplay",
        SongLuaOverlayKind::GraphDisplay { .. } => "GraphDisplay",
        SongLuaOverlayKind::Quad => "Quad",
    }
}

fn compare_layers(trace: &NativeTrace, compiled: &[CompiledSongLua], parity: &mut Parity) {
    parity.section("layer order");
    let definitions = trace
        .actor_definitions
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect::<HashMap<_, _>>();
    parity.check(trace.roots.len() == compiled.len(), || {
        format!(
            "root layer count differs: ITGmania has {}, DeadSync has {}",
            trace.roots.len(),
            compiled.len()
        )
    });
    for (layer, (root_id, compiled)) in trace.roots.iter().zip(compiled).enumerate() {
        let Some(root) = definitions.get(root_id.as_str()).copied() else {
            parity.check(false, || {
                format!("native layer {layer} has no actor-definition root")
            });
            continue;
        };
        let mut instances = Vec::new();
        collect_native_drawable_definitions(trace, root, &definitions, &mut instances);
        let native = instances
            .iter()
            .map(|actor| (actor.class, actor.name))
            .collect::<Vec<_>>();
        let deadsync = compiled
            .overlays
            .iter()
            .filter(|overlay| !matches!(kind_name(&overlay.kind), "Actor" | "ActorFrame" | "Sound"))
            .map(|overlay| (kind_name(&overlay.kind), overlay.name.as_deref()))
            .collect::<Vec<_>>();
        // Lua pairs() does not promise the declaration order of texture caches.
        // Retain every resource identity, but only ignore order when both sides
        // prove that a uniquely named actor stays invisible and is not proxied.
        let hidden = instances
            .iter()
            .filter(|actor| {
                actor.name.is_some()
                    && native
                        .iter()
                        .filter(|key| **key == (actor.class, actor.name))
                        .count()
                        == 1
                    && native_never_visible(trace, actor.id)
                    && compiled
                        .overlays
                        .iter()
                        .enumerate()
                        .any(|(index, overlay)| {
                            (kind_name(&overlay.kind), overlay.name.as_deref())
                                == (actor.class, actor.name)
                                && compiled_never_visible(compiled, index)
                        })
            })
            .map(|actor| (actor.class, actor.name))
            .collect::<HashSet<_>>();
        let mut native_members = native.clone();
        let mut actual_members = deadsync.clone();
        native_members.sort_unstable();
        actual_members.sort_unstable();
        let native_order = native
            .iter()
            .filter(|key| !hidden.contains(*key))
            .collect::<Vec<_>>();
        let actual_order = deadsync
            .iter()
            .filter(|key| !hidden.contains(*key))
            .collect::<Vec<_>>();
        parity.check(native_members == actual_members && native_order == actual_order, || {
            let first = native_order
                .iter()
                .zip(&actual_order)
                .position(|(native, deadsync)| native != deadsync)
                .unwrap_or_else(|| native_order.len().min(actual_order.len()));
            format!(
                "layer {layer} drawable membership/order differs: ITGmania has {}, DeadSync has {}; first visible difference at {first}: {:?} vs {:?}",
                native.len(),
                deadsync.len(),
                native_order.get(first),
                actual_order.get(first)
            )
        });
    }
}

fn native_never_visible(trace: &NativeTrace, id: &str) -> bool {
    trace
        .runtime_actors
        .iter()
        .find(|actor| actor.id == id)
        .is_some_and(|actor| {
            actor
                .render_state_samples
                .first()
                .is_some_and(|sample| sample.0 == 0 && !sample.2)
                && actor.render_state_samples.iter().all(|sample| !sample.2)
                && actor
                    .final_render_state
                    .as_ref()
                    .is_some_and(|state| !state.visible)
        })
        && !trace.operation_tracks.iter().any(|track| {
            track.operation == "ActorProxy.SetTarget"
                && track.samples.iter().any(|sample| {
                    sample
                        .3
                        .first()
                        .and_then(|value| value.get("actor"))
                        .and_then(Value::as_str)
                        == Some(id)
                })
        })
        && !trace
            .tween_tracks
            .iter()
            .flat_map(|track| &track.segments)
            .flat_map(|segment| &segment.operations)
            .any(|operation| {
                operation.operation == "ActorProxy.SetTarget"
                    && operation
                        .args
                        .first()
                        .and_then(|value| value.get("actor"))
                        .and_then(Value::as_str)
                        == Some(id)
            })
}

fn compiled_never_visible(compiled: &CompiledSongLua, index: usize) -> bool {
    let overlay = &compiled.overlays[index];
    let turns_visible = |target, value: &SongLuaOverlayUpdateValue| {
        target == SongLuaOverlayUpdateTarget::Visible
            && !matches!(value, SongLuaOverlayUpdateValue::Bool(false))
    };
    !overlay.initial_state.visible
        && !overlay
            .message_commands
            .iter()
            .flat_map(|command| &command.blocks)
            .any(|block| block.delta.visible == Some(true))
        && !compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == index
                && (ease.from.visible == Some(true) || ease.to.visible == Some(true))
        })
        && !compiled.overlay_updates.iter().any(|update| {
            update.overlay_index == index
                && update
                    .samples
                    .iter()
                    .any(|sample| turns_visible(update.target, &sample.value))
        })
        && !compiled
            .stateful_message_captures
            .iter()
            .flat_map(|capture| &capture.writes)
            .any(|write| write.overlay_index == index && turns_visible(write.target, &write.value))
        && !compiled.overlays.iter().any(|overlay| {
            matches!(
                overlay.kind,
                SongLuaOverlayKind::ActorProxy {
                    target: deadsync_assets::song_lua::SongLuaProxyTarget::Actor { overlay_index }
                } if overlay_index == index
            )
        })
}

#[derive(Clone, Copy)]
struct NativeFinalRenderState {
    alpha: f32,
    visible: bool,
    wrote_alpha: bool,
    wrote_visible: bool,
    sampled: bool,
}

impl Default for NativeFinalRenderState {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            visible: true,
            wrote_alpha: false,
            wrote_visible: false,
            sampled: false,
        }
    }
}

#[derive(Clone, Copy)]
struct NativeInstance<'a> {
    id: &'a str,
    class: &'a str,
    name: Option<&'a str>,
}

fn collect_native_drawable_definitions<'a>(
    trace: &'a NativeTrace,
    parent: &'a NativeDefinition,
    definitions: &HashMap<&'a str, &'a NativeDefinition>,
    out: &mut Vec<NativeInstance<'a>>,
) {
    let actor = parent
        .runtime_actors
        .first()
        .map_or(parent.id.as_str(), String::as_str);
    collect_native_instances(trace, parent, actor, definitions, true, out);
}

fn collect_native_instances<'a>(
    trace: &'a NativeTrace,
    parent: &'a NativeDefinition,
    actor: &'a str,
    definitions: &HashMap<&'a str, &'a NativeDefinition>,
    drawables_only: bool,
    out: &mut Vec<NativeInstance<'a>>,
) {
    let draw_order = trace.draw_orders.iter().find(|order| {
        order.parent_actor == actor
            || (order.parent_actor.is_empty()
                && order.parent_definition_id == parent.id
                && order.instance == 1)
    });
    let mut source_children = parent.children.iter().collect::<Vec<_>>();
    source_children.sort_by_key(|child| child.layer_index);
    let children = draw_order
        .map(|order| {
            let mut children = order.final_children.iter().collect::<Vec<_>>();
            if !drawables_only {
                children.sort_by_key(|child| child.layer_index);
            }
            children
                .into_iter()
                .map(|child| (child.definition_id.as_str(), child.actor.as_deref()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            source_children
                .iter()
                .map(|child| (child.definition_id.as_str(), None))
                .collect()
        });
    for (child, actor) in children {
        let Some(definition) = definitions.get(child).copied() else {
            continue;
        };
        let id = actor.unwrap_or_else(|| {
            definition
                .runtime_actors
                .first()
                .map_or(definition.id.as_str(), String::as_str)
        });
        let include = if drawables_only {
            !matches!(definition.class.as_str(), "Actor" | "ActorFrame" | "Sound")
        } else {
            definition.class != "Actor" || !definition.children.is_empty()
        };
        if include {
            out.push(NativeInstance {
                id,
                class: &definition.class,
                name: definition.name.as_deref(),
            });
        }
        collect_native_instances(trace, definition, id, definitions, drawables_only, out);
    }
}

fn native_color_alpha(args: &[Value]) -> Option<f32> {
    args.first()
        .and_then(Value::as_array)
        .and_then(|color| value_f32(color.get(3)))
        .or_else(|| value_f32(args.get(3)))
}

fn native_final_render_state(trace: &NativeTrace, actor: &str) -> NativeFinalRenderState {
    if let Some(snapshot) = trace
        .runtime_actors
        .iter()
        .find(|runtime| runtime.id == actor)
        .and_then(|runtime| runtime.final_render_state.as_ref())
    {
        return NativeFinalRenderState {
            alpha: snapshot.alpha.unwrap_or(1.0),
            visible: snapshot.visible,
            wrote_alpha: snapshot.alpha.is_some(),
            wrote_visible: true,
            sampled: true,
        };
    }
    let mut operations = trace
        .tween_tracks
        .iter()
        .filter(|track| track.actor == actor)
        .flat_map(|track| &track.segments)
        .flat_map(|segment| &segment.operations)
        .map(|operation| (operation.seq, operation.operation.as_str(), &operation.args))
        .chain(
            trace
                .operation_tracks
                .iter()
                .filter(|track| track.actor == actor)
                .flat_map(|track| {
                    track
                        .samples
                        .iter()
                        .map(|(seq, _, _, args)| (*seq, track.operation.as_str(), args))
                }),
        )
        .collect::<Vec<_>>();
    operations.sort_by_key(|(seq, _, _)| *seq);
    let mut state = NativeFinalRenderState::default();
    for (_, operation, args) in operations {
        let method = operation
            .rsplit('.')
            .next()
            .unwrap_or(operation)
            .to_ascii_lowercase();
        match method.as_str() {
            "diffusealpha" => {
                if let Some(alpha) = value_f32(args.first()) {
                    state.alpha = alpha;
                    state.wrote_alpha = true;
                }
            }
            "diffuse" => {
                if let Some(alpha) = native_color_alpha(args) {
                    state.alpha = alpha;
                    state.wrote_alpha = true;
                }
            }
            "visible" => {
                if let Some(visible) = args.first().and_then(Value::as_bool) {
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
            progress: None,
            queued: false,
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

// Older fixtures contain setter destinations without a current-state snapshot.
fn compiled_dest_render_state(
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
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    parity.section("final render");
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
        let seconds = trace.end_position.seconds;
        let beat = trace
            .end_position
            .beat
            .unwrap_or_else(|| song_beat_at_elapsed_seconds(seconds, context));
        let final_states = compiled_local_states_at(compiled, context, beat, seconds);
        for (definition, overlay_index) in native.into_iter().zip(deadsync) {
            let expected = native_final_render_state(trace, definition.id);
            let actual = if expected.sampled {
                final_states[overlay_index]
            } else {
                compiled_dest_render_state(compiled, overlay_index)
            };
            if expected.wrote_alpha {
                parity.check((expected.alpha - actual.diffuse[3]).abs() <= EPSILON, || {
                    format!(
                        "layer {layer} final alpha differs for {}/{}: ITGmania {:.4}, DeadSync {:.4}",
                        definition.id, definition.class, expected.alpha, actual.diffuse[3]
                    )
                });
            }
            if expected.wrote_visible {
                parity.check(expected.visible == actual.visible, || {
                    format!(
                        "layer {layer} final visibility differs for {}/{}: ITGmania {}, DeadSync {}",
                        definition.id, definition.class, expected.visible, actual.visible
                    )
                });
            }
        }
    }
}

#[test]
fn shared_actor_instances_match_native_rendering() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/shared-actor.json"));
    let simfile = root.join("crates/deadsync-song-lua/tests/fixtures/shared-actor.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    assert_eq!(parity.checks(), 20);
    parity.assert_complete(&trace.title);
}

#[test]
fn recurring_ease_tables_match_native_shared_state() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/recurring-ease-table.json"),
    );
    let mut context = SongLuaCompileContext::new(&song_dir, trace.title.clone());
    context.screen_width = trace.display.logical_width;
    context.screen_height = trace.display.logical_height;
    context.music_length_seconds = trace.end_position.seconds;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(
        &[song_dir.join("recurring-ease-table.lua").as_path()],
        0,
        &context,
    )
    .expect("compile recurring ease tables");
    let mut parity = compare_semantics(&trace, &compiled, 0, &context);
    assert_eq!(parity.checks(), 270);
    parity.section("shared ease state");
    for track in &trace.operation_tracks {
        let target = match track.operation.as_str() {
            "Quad.x" => SongLuaOverlayUpdateTarget::X,
            "Quad.y" => SongLuaOverlayUpdateTarget::Y,
            _ => continue,
        };
        let index = if track.actor == "def-0002" { 0 } else { 1 };
        for (_, beat, seconds, args) in &track.samples {
            let state = compiled_local_states_at(&compiled[0], &context, *beat, *seconds)[index];
            let actual = if target == SongLuaOverlayUpdateTarget::X {
                state.x
            } else {
                state.y
            };
            let expected = value_f32(args.first()).expect("native scalar write");
            parity.check((actual - expected).abs() < 1e-5, || {
                format!(
                    "{} {} at {beat}: expected {expected}, got {actual:?}",
                    track.actor, track.operation
                )
            });
        }
    }
    assert_eq!(parity.checks(), 342);
    parity.assert_complete("recurring ease table shared state");
}

#[test]
fn position_spline_curves_match_native_cpp() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let native: Value = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-song-lua-micro/position-spline-native.json"))
            .expect("native Position fixture"),
    )
    .expect("valid native Position JSON");
    assert_eq!(native["oracle"], "itgmania_native_actor_conformance");
    let mut checks = 0;
    for case in native["splines"].as_array().expect("native spline cases") {
        let points: Vec<[f32; 3]> = case["points"]
            .as_array()
            .expect("points")
            .iter()
            .map(|point| std::array::from_fn(|axis| point[axis].as_f64().expect("axis") as f32))
            .collect();
        let data = deadsync_gameplay::SongLuaSplineData {
            coefficients: deadsync_gameplay::solve_song_lua_spline(&points).into(),
            constant: points.iter().all(|point| *point == points[0]),
            beats_per_t: case["beats_per_t"].as_f64().expect("beats per t") as f32,
            receptor_t: case["receptor_t"].as_f64().expect("receptor t") as f32,
            subtract_song_beat: case["subtract_song_beat"].as_bool().expect("beat mode"),
        };
        for sample in case["samples"].as_array().expect("samples") {
            let song = sample["song_beat"].as_f64().expect("song beat") as f32;
            let note = sample["note_beat"].as_f64().expect("note beat") as f32;
            let (position, derivative) = data.view().sample(song, note);
            for (key, actual) in [
                ("position", position),
                ("derivative", derivative),
                ("receptor", data.view().receptor(song)),
            ] {
                for axis in 0..3 {
                    let expected = sample[key][axis].as_f64().expect("native axis") as f32;
                    assert!(
                        (actual[axis] - expected).abs() <= 0.0002,
                        "{key} axis {axis}, song {song}, note {note}: {} vs {expected}",
                        actual[axis]
                    );
                    checks += 1;
                }
            }
        }
    }
    assert_eq!(checks, 6426);
}

#[test]
fn position_spline_tracks_keep_native_clock_and_modifier_state() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/position-spline.json"));
    let mut context = SongLuaCompileContext::new(&song_dir, trace.title.clone());
    context.music_length_seconds = trace.end_position.seconds;
    context.screen_width = trace.display.logical_width;
    context.screen_height = trace.display.logical_height;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(
        &[song_dir.join("position-spline.lua").as_path()],
        0,
        &context,
    )
    .expect("compile Position splines");
    assert_eq!(compiled[0].column_splines.len(), 1);
    let track = &compiled[0].column_splines[0];
    assert!(
        track
            .at_second(0.99)
            .expect("initial frame")
            .position
            .is_none()
    );
    for (second, first_y) in [(1.5, -135.0), (2.5, -115.0)] {
        let frame = track.at_second(second).expect("active frame");
        assert_eq!(
            frame.position.as_ref().expect("Position").coefficients[0][1][0],
            first_y
        );
        assert_eq!(frame.position.as_ref().expect("Position").beats_per_t, 0.5);
        assert_eq!(frame.position.as_ref().expect("Position").receptor_t, 0.25);
        assert_eq!(frame.zoom.as_ref().expect("Zoom").coefficients.len(), 4);
    }
    assert!(
        track
            .at_second(3.1)
            .expect("disabled frame")
            .position
            .is_none()
    );
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.75,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..Default::default()
        },
        &[],
    );
    let shifted =
        deadsync_song_lua::gameplay::song_lua_column_spline_tracks(&compiled[0], 0, &timing, 0.1);
    let shift = timing.get_time_for_beat_exact(0.0) - 0.1;
    assert_eq!(
        shifted[0]
            .at_second(1.5 + shift)
            .expect("shifted frame")
            .position,
        track.at_second(1.5).expect("source frame").position
    );
    let mut parity = compare_semantics(&trace, &compiled, 0, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    parity.assert_complete("Position spline clock and modifier state");
}

#[test]
fn aft_boundaries_match_native_geometry_and_visibility() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/aft-boundary.json"));
    let native: Value = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-song-lua-micro/aft-boundary-native.json"))
            .unwrap(),
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, trace.title.clone());
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    context.music_length_seconds = trace.end_position.seconds;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[song_dir.join("aft-boundary.lua").as_path()], 0, &context)
            .unwrap();
    let mut parity = compare_semantics(&trace, &compiled, 0, &context);
    let drawables = projected_drawable_map(&trace, &compiled);
    parity.section("native AFT vertices");
    for sample in native["samples"].as_array().unwrap() {
        let second = value_f32(sample.get("time")).unwrap();
        let states = compiled_overlay_states_at(&compiled[0], &context, second, second);
        for actor in sample["actors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|actor| actor["kind"] == "sprite")
        {
            let definition = trace
                .actor_definitions
                .iter()
                .find(|def| actor["name"].as_str() == def.name.as_deref())
                .unwrap();
            let (_, index) = drawables[&definition.id];
            let state = states[index];
            parity.check(state.visible == actor["visible"].as_bool().unwrap(), || {
                format!("{:?} visibility", definition.name)
            });
            if actor["visible"] == false {
                continue;
            }
            let track = trace
                .projected_vertex_tracks
                .iter()
                .find(|track| track.definition_id.as_deref() == Some(&definition.id))
                .unwrap();
            let vertices = if track.camera_actor == "orthographic-screen" {
                compiled_world_vertices(state, track.texture_size).map(|[x, y, _, _]| [x, y])
            } else {
                compiled_perspective_vertices(
                    &compiled[0],
                    &states,
                    index,
                    state,
                    track.texture_size,
                    None,
                )
                .unwrap()
            };
            for (corner, actual) in vertices.iter().enumerate() {
                let expected = &actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]]["screen"];
                for axis in 0..2 {
                    let expected = value_f32(expected.get(axis)).unwrap();
                    parity.check((actual[axis] - expected).abs() < 0.0002, || {
                        format!(
                            "{:?} corner {corner} axis {axis}: {} vs {expected}",
                            definition.name, actual[axis]
                        )
                    });
                }
            }
        }
    }
    parity.assert_complete("native AFT boundary");
}

#[test]
fn final_render_samples_unfinished_native_fade() {
    crate::paths::init();
    let song_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&song_dir, "Background Fit");
    context.screen_width = 854.0;
    context.music_length_seconds = 4.0;
    let compiled = deadsync_assets::song_lua::compile_song_lua(
        &song_dir.join("background-fit-smooth.lua"),
        &context,
    )
    .unwrap();
    let native: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/itgmania-actors/background-fit-smooth.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let alpha = native["samples"][3]["actors"][3]["current"]["diffuse"][0][3]
        .as_f64()
        .unwrap();
    assert!((alpha - 0.84375).abs() < 1e-6);
    let trace: NativeTrace = serde_json::from_value(serde_json::json!({
        "oracle": "itgmania_song_lua_headless_semantic_trace", "title": "Background Fit",
        "style": "single", "simfile": "background-fit-smooth.lua", "roots": ["root"],
        "actor_definitions": [
            {"id": "root", "class": "ActorFrame", "children": [
                {"layer_index": 0, "definition_id": "fit"},
                {"layer_index": 1, "definition_id": "cover"}
            ]},
            {"id": "fit", "class": "Sprite", "name": "Fit"},
            {"id": "cover", "class": "Quad", "name": "Cover"}
        ],
        "runtime_actors": [
            {"id": "fit", "path": "Fit", "final_render_state": {"alpha": 1, "visible": true}},
            {"id": "cover", "path": "Cover", "final_render_state": {"alpha": alpha, "visible": true}}
        ],
        "timeline_tracks": [], "tween_tracks": [],
        "end_position": {"seconds": 151.0 / 60.0}, "trace_until_beat": 151.0 / 60.0,
        "fixture_context": {"beat_step": 0.25},
        "display": {"width": 854, "height": 480, "logical_width": 854, "logical_height": 480}
    })).unwrap();
    let mut parity = Parity::default();
    compare_final_render_states(&trace, &[compiled], &context, &mut parity);
    assert_eq!(parity.checks(), 4);
    parity.assert_complete("unfinished native fade");
}

fn compare_player_proxy_sources(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    parity: &mut Parity,
) {
    parity.section("player proxy sources");
    let drawables = projected_drawable_map(trace, compiled);
    for track in &trace.player_render_tracks {
        let proxies = track
            .samples
            .iter()
            .flat_map(|sample| &sample.5)
            .collect::<HashSet<_>>();
        if proxies.is_empty() {
            continue;
        }
        let Some(player) = track.player.checked_sub(1).filter(|&player| player < 2) else {
            continue;
        };
        for proxy in proxies {
            let definition = trace.actor_definitions.iter().find(|definition| {
                definition.id == *proxy || definition.runtime_actors.contains(proxy)
            });
            let actual = definition
                .and_then(|definition| drawables.get(&definition.id))
                .map(|&(layer, index)| &compiled[layer].overlays[index].kind);
            parity.check(
                matches!(actual, Some(SongLuaOverlayKind::ActorProxy {
                target: deadsync_assets::song_lua::SongLuaProxyTarget::Player { player_index }
            }) if *player_index == player),
                || {
                    format!(
                        "PlayerP{} proxy {proxy} has a different or missing target: {actual:?}",
                        track.player
                    )
                },
            );
        }
        let Some(actor) = trace
            .external_actors
            .iter()
            .find(|actor| actor.path == track.path)
        else {
            continue;
        };
        let writes_alpha = |operation: &str| {
            matches!(
                operation
                    .rsplit('.')
                    .next()
                    .unwrap_or(operation)
                    .to_ascii_lowercase()
                    .as_str(),
                "diffuse" | "diffusealpha"
            )
        };
        let changes_alpha = trace
            .operation_tracks
            .iter()
            .any(|track| track.actor == actor.id && writes_alpha(&track.operation))
            || trace
                .tween_tracks
                .iter()
                .filter(|track| track.actor == actor.id)
                .any(|track| {
                    track
                        .segments
                        .iter()
                        .flat_map(|segment| &segment.operations)
                        .any(|operation| writes_alpha(&operation.operation))
                });
        // A native Player with no alpha writes stays opaque, even when its
        // original is hidden. ActorProxy overrides visibility, not alpha.
        if !changes_alpha {
            for (layer, compiled) in compiled.iter().enumerate() {
                let alpha = compiled.player_actors[player].initial_state.diffuse[3];
                parity.check((alpha - 1.0).abs() <= EPSILON, || format!(
                    "layer {layer} PlayerP{} proxy source alpha differs: ITGmania 1.000, DeadSync {alpha:.3}", track.player
                ));
            }
        }
    }
}

fn native_update_render_writes(
    trace: &NativeTrace,
    actor: &str,
) -> (Vec<(f32, f32)>, Vec<(f32, bool)>) {
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

fn native_render_probe(
    trace: &NativeTrace,
    actor: &str,
    beat: f32,
) -> Option<(f32, Option<f32>, bool)> {
    let samples = &trace
        .runtime_actors
        .iter()
        .find(|runtime| runtime.id == actor)?
        .render_state_samples;
    let frame = trace
        .update_frames
        .partition_point(|&(position, _)| position < f64::from(beat) - 0.000001);
    let &(position, _) = trace.update_frames.get(frame)?;
    let next = samples.partition_point(|&(index, _, _)| index <= frame);
    let &(_, alpha, visible) = samples.get(next.checked_sub(1)?)?;
    Some((position as f32, alpha, visible))
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
    context: &SongLuaCompileContext,
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    parity: &mut Parity,
) {
    parity.section("render persistence");
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
            let (alpha_writes, visible_writes) = native_update_render_writes(trace, definition.id);
            for (beat, expected) in persistence_probes(
                &alpha_writes,
                trace.fixture_context.beat_step,
                trace.trace_until_beat,
            ) {
                let (beat, expected) = match native_render_probe(trace, definition.id, beat) {
                    Some((beat, Some(alpha), _)) => (beat, alpha),
                    Some((_, None, _)) => continue,
                    None => (beat, expected),
                };
                let Some(SongLuaOverlayUpdateValue::Vec4(actual)) = compiled_update_value_at(
                    context,
                    compiled,
                    overlay_index,
                    SongLuaOverlayUpdateTarget::Diffuse,
                    beat,
                    song_elapsed_seconds_at(beat, context),
                ) else {
                    continue;
                };
                let actual = actual[3];
                parity.check((expected - actual).abs() <= 0.03, || {
                    format!(
                        "layer {layer} alpha persistence differs for {}/{} at beat {beat:.3}: ITGmania {expected:.4}, DeadSync {actual:.4}",
                        definition.id, definition.class
                    )
                });
            }
            for (beat, expected) in persistence_probes(
                &visible_writes,
                trace.fixture_context.beat_step,
                trace.trace_until_beat,
            ) {
                let (beat, expected) = native_render_probe(trace, definition.id, beat)
                    .map_or((beat, expected), |(beat, _, visible)| (beat, visible));
                let Some(SongLuaOverlayUpdateValue::Bool(actual)) = compiled_update_value_at(
                    context,
                    compiled,
                    overlay_index,
                    SongLuaOverlayUpdateTarget::Visible,
                    beat,
                    song_elapsed_seconds_at(beat, context),
                ) else {
                    continue;
                };
                parity.check(expected == actual, || {
                    format!(
                        "layer {layer} visibility persistence differs for {}/{} at beat {beat:.3}: ITGmania {expected}, DeadSync {actual}",
                        definition.id, definition.class
                    )
                });
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
    relative: bool,
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
        "x" | "addx" => scalar(Target::X, 0),
        "y" | "addy" => scalar(Target::Y, 0),
        "z" | "addz" => scalar(Target::Z, 0),
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
        "rotationx" | "baserotationx" | "addrotationx" => scalar(Target::RotationX, 0),
        "rotationy" | "baserotationy" | "addrotationy" => scalar(Target::RotationY, 0),
        "rotationz" | "baserotationz" | "addrotationz" => scalar(Target::RotationZ, 0),
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

fn native_update_render_writes_all(trace: &NativeTrace, actor: &str) -> Vec<NativeRenderWrite> {
    let mut writes =
        trace
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
                            relative: operation.operation.rsplit('.').next().is_some_and(
                                |method| {
                                    matches!(
                                        method.to_ascii_lowercase().as_str(),
                                        "addx"
                                            | "addy"
                                            | "addz"
                                            | "addrotationx"
                                            | "addrotationy"
                                            | "addrotationz"
                                    )
                                },
                            ),
                        })
                })
            })
            .collect::<Vec<_>>();
    writes.sort_by_key(|write| write.seq);
    let mut merged = Vec::<NativeRenderWrite>::with_capacity(writes.len());
    for mut write in writes {
        if let Some(index) = merged.iter().position(|current| {
            current.target == write.target && (current.beat - write.beat).abs() <= EPSILON
        }) {
            // Compare the completed frame: y(base); addy(offset) leaves their
            // sum, not the earlier setter argument. Unanchored additions have
            // no absolute value here and remain covered by projected geometry.
            if write.relative
                && let (
                    SongLuaOverlayUpdateValue::F32(base),
                    SongLuaOverlayUpdateValue::F32(offset),
                ) = (&merged[index].value, &mut write.value)
            {
                *offset += *base;
            }
            merged[index] = write;
        } else if !write.relative {
            merged.push(write);
        }
    }
    merged.sort_by(|left, right| left.beat.total_cmp(&right.beat));
    merged
}

fn overlay_update_time(
    context: &SongLuaCompileContext,
    unit: SongLuaTimeUnit,
    beat: f32,
    seconds: f32,
) -> f32 {
    match unit {
        SongLuaTimeUnit::Beat => beat,
        SongLuaTimeUnit::Second => {
            seconds * deadsync_song_lua::song_music_rate(context)
                + context
                    .song_timing
                    .as_ref()
                    .map_or(0.0, |timing| timing.get_time_for_beat_exact(0.0))
        }
        SongLuaTimeUnit::BeatClock => seconds * deadsync_song_lua::song_music_rate(context),
    }
}

fn compiled_update_value_at(
    context: &SongLuaCompileContext,
    compiled: &CompiledSongLua,
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    beat: f32,
    seconds: f32,
) -> Option<SongLuaOverlayUpdateValue> {
    let time = overlay_update_time(context, compiled.overlay_update_unit, beat, seconds);
    let samples = &compiled
        .overlay_updates
        .iter()
        .rev()
        .find(|track| {
            track.overlay_index == overlay_index
                && track.target == target
                && track
                    .samples
                    .first()
                    .is_some_and(|sample| sample.time <= time)
        })?
        .samples;
    sample_update_track(
        context,
        compiled.overlay_update_unit,
        target,
        samples,
        beat,
        seconds,
    )
}

fn sample_update_track(
    context: &SongLuaCompileContext,
    unit: SongLuaTimeUnit,
    target: SongLuaOverlayUpdateTarget,
    samples: &[deadsync_song_lua::SongLuaOverlayUpdateSample],
    beat: f32,
    seconds: f32,
) -> Option<SongLuaOverlayUpdateValue> {
    let time = overlay_update_time(context, unit, beat, seconds);
    let next_index = samples.partition_point(|sample| sample.time <= time);
    if next_index == 0 {
        return None;
    }
    if matches!(
        target,
        SongLuaOverlayUpdateTarget::EffectTime | SongLuaOverlayUpdateTarget::BaseRotation
    ) {
        return Some(samples[next_index - 1].value.clone());
    }
    let current = &samples[next_index - 1];
    let Some(next) = samples.get(next_index) else {
        return Some(current.value.clone());
    };
    let span = next.time - current.time;
    // The native oracle serializes its 60 Hz clock as f64 while runtime tracks
    // store coordinates as f32. Compare the same frame directly when the
    // two representations straddle it. Dense tracks contain one sample per
    // update frame; sparse tween tracks must retain interpolation semantics.
    let nearest = [current, next].into_iter().min_by(|left, right| {
        (left.time - time)
            .abs()
            .total_cmp(&(right.time - time).abs())
    })?;
    let frame_epsilon = if unit == SongLuaTimeUnit::Beat {
        EPSILON
    } else {
        0.000_1
    };
    if (nearest.time - time).abs() <= frame_epsilon {
        return Some(nearest.value.clone());
    }
    if span <= f32::EPSILON {
        return Some(next.value.clone());
    }
    // Gameplay converts captured beat positions to seconds before sampling.
    // Beat interpolation changes a seconds-based fade across a BPM boundary.
    let (start, end, now) = if unit == SongLuaTimeUnit::Beat {
        (
            song_elapsed_seconds_at(current.time, context),
            song_elapsed_seconds_at(next.time, context),
            seconds,
        )
    } else {
        (current.time, next.time, time)
    };
    let t = if end <= start + f32::EPSILON {
        1.0
    } else {
        (now - start) / (end - start)
    };
    Some(current.value.lerp(&next.value, t))
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
        Target::EffectTime => state
            .effect_time
            .map_or(UpdateValue::None, UpdateValue::Vec2),
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
    trace: &'a NativeTrace,
    parent: &'a NativeDefinition,
    definitions: &HashMap<&'a str, &'a NativeDefinition>,
    out: &mut Vec<NativeInstance<'a>>,
) {
    let actor = parent
        .runtime_actors
        .first()
        .map_or(parent.id.as_str(), String::as_str);
    collect_native_instances(trace, parent, actor, definitions, false, out);
}

fn compare_update_render_values(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    parity.section("update values");
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
        collect_native_overlay_definitions(trace, root, &definitions, &mut native);
        let native_len = native.len();
        let overlay_indices = (0..compiled.overlays.len())
            .filter(|index| {
                Some(*index) != compiled.screen_overlay_index
                    && !matches!(
                        compiled.overlays[*index].kind,
                        SongLuaOverlayKind::WrapperState
                    )
                    // Native collection omits leaf Actors. Lua can retain one
                    // as a tween variable without adding a drawable actor.
                    && (!matches!(compiled.overlays[*index].kind, SongLuaOverlayKind::Actor)
                        || compiled.overlays.iter().any(|actor| actor.parent_index == Some(*index)))
            })
            .collect::<Vec<_>>();
        let pairs = if native_len == overlay_indices.len() {
            native
                .into_iter()
                .zip(overlay_indices)
                .map(|(definition, index)| (index, definition))
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
            parity.check(false, || {
                format!(
                    "layer {layer} update comparison topology differs: ITGmania has {} non-root actors, DeadSync has {} overlays",
                    native_len,
                    compiled.overlays.len()
                )
            });
            continue;
        }
        for (overlay_index, definition) in pairs {
            for write in native_update_render_writes_all(trace, definition.id) {
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
                let actual = compiled
                    .overlay_writes
                    .iter()
                    .find(|track| {
                        track.overlay_index == overlay_index && track.target == write.target
                    })
                    .and_then(|track| {
                        track
                            .samples
                            .iter()
                            .find(|sample| (sample.time - write.beat).abs() <= EPSILON)
                    })
                    .map(|sample| sample.value.clone())
                    .or_else(|| {
                        // A queued startup setter precedes the first tween advance.
                        // Compare its captured argument, rather than the draw state
                        // after that same frame has already consumed its delta.
                        compiled
                            .messages
                            .iter()
                            .filter(|event| {
                                event.message == "__songlua_queued_startup"
                                    && (event.beat - write.beat).abs() <= EPSILON
                            })
                            .find_map(|event| {
                                compiled.overlays[overlay_index]
                                    .message_commands
                                    .iter()
                                    .find(|command| command.message == event.message)?
                                    .blocks
                                    .iter()
                                    .rev()
                                    .find_map(|block| {
                                        (block.duration == 0.0
                                            && block.start <= 0.0
                                            && block.delta.has_update_target(write.target))
                                        .then(|| {
                                            let state = overlay_state_after_blocks(
                                                SongLuaOverlayState::default(),
                                                [block],
                                                0.0,
                                            );
                                            overlay_state_render_value(&state, write.target)
                                        })
                                        .flatten()
                                    })
                            })
                    })
                    .or_else(|| {
                        compiled_update_value_at(
                            context,
                            compiled,
                            overlay_index,
                            write.target,
                            write.beat,
                            song_elapsed_seconds_at(write.beat, context),
                        )
                    })
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
                    parity.check(false, || {
                        format!(
                            "layer {layer} missing {:?} state for {}/{} at beat {:.3}",
                            write.target, definition.id, definition.class, write.beat
                        )
                    });
                    continue;
                };
                parity.check(render_value_matches(&write.value, &actual), || {
                    format!(
                        "layer {layer} {:?} differs for {}/{} at beat {:.3}: ITGmania {:?}, DeadSync {:?}",
                        write.target,
                        definition.id,
                        definition.class,
                        write.beat,
                        write.value,
                        actual
                    )
                });
            }
        }
    }
}

#[test]
fn update_matching_keeps_retained_actor_variables() {
    let trace: NativeTrace = serde_json::from_value(serde_json::json!({
        "oracle": "itgmania_native_actor_conformance", "title": "actor variable", "style": "single",
        "simfile": "", "roots": ["root"], "runtime_actors": [],
        "actor_definitions": [
            {"id": "root", "class": "ActorFrame", "children": [
                {"layer_index": 1, "definition_id": "aux"},
                {"layer_index": 2, "definition_id": "mover"}
            ]},
            {"id": "aux", "class": "Actor"},
            {"id": "mover", "class": "ActorFrame"}
        ],
        "timeline_tracks": [],
        "tween_tracks": [{"actor": "mover", "command": "UpdateCommand", "kind": "immediate",
            "segments": [{"enqueue_seq": 1, "beat": 0, "duration": 0,
                "operations": [{"seq": 2, "operation": "ActorFrame.x", "args": [13]}]}]}],
        "end_position": {"seconds": 1.0},
        "display": {"width": 640, "height": 480, "logical_width": 640, "logical_height": 480},
        "fixture_context": {"beat_step": 0.25}, "trace_until_beat": 1.0
    }))
    .expect("actor variable reference");
    let mut compiled = CompiledSongLua {
        overlays: [SongLuaOverlayKind::Actor, SongLuaOverlayKind::ActorFrame]
            .into_iter()
            .map(|kind| deadsync_song_lua::SongLuaOverlayActor {
                kind,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    x: 13.0,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            })
            .collect(),
        ..CompiledSongLua::default()
    };
    let context = SongLuaCompileContext::new("", "actor variable");
    let mut parity = Parity::default();
    compare_update_render_values(
        &trace,
        std::slice::from_ref(&compiled),
        &context,
        &mut parity,
    );
    assert_eq!(parity.checks(), 1, "retain the native transform comparison");
    parity.assert_complete("actor variable");
    compiled.overlays[1].initial_state.x = 0.0;
    let mut regression = Parity::default();
    compare_update_render_values(&trace, &[compiled], &context, &mut regression);
    assert_eq!(
        regression.gaps.len(),
        1,
        "incorrect transforms must still fail"
    );
}

fn compare_timeline(trace: &NativeTrace, compiled: &CompiledSongLua, parity: &mut Parity) {
    parity.section("timeline");
    let beat_epsilon = trace.fixture_context.beat_step + EPSILON;
    for track in &trace.timeline_tracks {
        for (_, beat, _, args, _) in &track.samples {
            let Some(beat) = beat else { continue };
            if track.kind == "message" {
                let Some(message) = args.first().and_then(Value::as_str) else {
                    continue;
                };
                let has_listener = compiled
                    .messages
                    .iter()
                    .any(|event| event.message == message)
                    || compiled
                        .stateful_message_captures
                        .iter()
                        .any(|capture| capture.message == message)
                    || compiled
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
                        .chain(
                            compiled
                                .screen_layers
                                .iter()
                                .flat_map(|actor| &actor.message_commands),
                        )
                        .any(|command| command.message == message);
                if !has_listener {
                    continue;
                }
                let found = compiled.messages.iter().any(|actual| {
                    actual.message == message && (actual.beat - beat).abs() <= beat_epsilon
                });
                parity.check(found, || {
                    format!(
                        "missing message `{message}` near beat {beat:.3} (operation {})",
                        track.operation
                    )
                });
            }
        }
    }
    // Recurring modifier readers compile to sampled numeric targets, not raw
    // beat_mods strings. The runtime modifier audit checks their gameplay values
    // for both PlayerOptions writes and PlayerState::SetPlayerOptions.
}

fn value_f32(value: Option<&Value>) -> Option<f32> {
    value.and_then(Value::as_f64).map(|value| value as f32)
}

fn projected_alpha(value: &Value) -> Option<f32> {
    value_f32(Some(value)).or_else(|| {
        if value.get("type")?.as_str()? != "number" {
            return None;
        }
        match value.get("value")?.as_str()? {
            "infinity" => Some(f32::INFINITY),
            "-infinity" => Some(f32::NEG_INFINITY),
            "nan" => Some(f32::NAN),
            _ => None,
        }
    })
}

fn value_u32(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn native_player_option_value_at(
    trace: &NativeTrace,
    player: usize,
    operation: &str,
    beat: f32,
) -> Option<f32> {
    let player_path = format!("player-state:PLAYER_{player}/options:ModsLevel_Song");
    trace
        .timeline_tracks
        .iter()
        .filter(|track| {
            track.kind == "modifier"
                && track.actor.as_deref() == Some(player_path.as_str())
                && track.operation == operation
        })
        .flat_map(|track| &track.samples)
        .filter(|(_, sample_beat, _, _, _)| sample_beat.is_some_and(|value| value <= beat))
        .max_by_key(|(sequence, _, _, _, _)| *sequence)
        .and_then(|(_, _, _, args, _)| value_f32(args.first()))
}

fn expected_block(track: &NativeTweenTrack, segment: &NativeTweenSegment) -> ExpectedBlock {
    // QueueCommand/QueueMessage and Sleep's implicit tail are instantaneous
    // states. Their native linear object performs no interpolation.
    let easing = match (track.kind.as_str(), track.easing.as_deref()) {
        ("command" | "message", _) => None,
        (_, Some("linear")) => Some("linear"),
        (_, Some("accelerate")) => Some("inQuad"),
        (_, Some("decelerate")) => Some("outQuad"),
        (_, Some("smooth")) => Some("smooth"),
        (_, Some("spring")) => Some("spring"),
        (_, Some("bouncebegin")) => Some("bouncebegin"),
        (_, Some("bounceend")) => Some("bounceend"),
        _ => None,
    };
    let mut block = ExpectedBlock {
        duration: segment.duration,
        easing: if segment.implicit { None } else { easing },
        sleep: track.kind == "sleep",
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

// Initial definition/instance parents retain ownership even after RemoveChild.
// Final draw order cannot locate actors whose earlier writes still need auditing.
fn native_actor_layers(trace: &NativeTrace) -> HashMap<&str, usize> {
    let parents = trace
        .actor_definitions
        .iter()
        .flat_map(|parent| {
            parent
                .children
                .iter()
                .map(move |child| (child.definition_id.as_str(), parent.id.as_str()))
        })
        .chain(trace.actor_definitions.iter().flat_map(|definition| {
            definition
                .runtime_actors
                .iter()
                .filter(|actor| **actor != definition.id)
                .map(|actor| (actor.as_str(), definition.id.as_str()))
        }))
        .chain(trace.runtime_actors.iter().filter_map(|actor| {
            actor
                .parent_id
                .as_deref()
                .map(|parent| (actor.id.as_str(), parent))
        }))
        .collect::<HashMap<_, _>>();
    let root_layers = trace
        .roots
        .iter()
        .enumerate()
        .map(|(index, root)| (root.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut layers = HashMap::new();
    for actor in trace
        .actor_definitions
        .iter()
        .map(|actor| actor.id.as_str())
        .chain(trace.runtime_actors.iter().map(|actor| actor.id.as_str()))
    {
        let mut ancestor = actor;
        for _ in 0..=parents.len() {
            if let Some(layer) = root_layers.get(ancestor).copied() {
                layers.insert(actor, layer);
                break;
            }
            let Some(parent) = parents.get(ancestor).copied() else {
                break;
            };
            ancestor = parent;
        }
    }
    layers
}

fn trace_commands(trace: &NativeTrace) -> Vec<ExpectedCommand> {
    let paths = trace
        .runtime_actors
        .iter()
        .map(|actor| (actor.id.as_str(), actor.path.as_str()))
        .collect::<HashMap<_, _>>();
    let actor_layers = native_actor_layers(trace);
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
            let layer = actor_layers.get(track.actor.as_str()).copied();
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
                    .map(|segment| (segment.enqueue_seq, expected_block(track, segment))),
            );
        }
    }
    for command in &mut out {
        command.blocks.sort_by_key(|(seq, _)| *seq);
        // Actor::SetVisible updates a non-tweened field immediately, even if
        // the recorder attaches its setter to the current tween segment.
        let immediate = command
            .blocks
            .iter_mut()
            .filter_map(|(seq, block)| {
                block.visible.take().map(|visible| {
                    (
                        *seq,
                        ExpectedBlock {
                            visible: Some(visible),
                            ..ExpectedBlock::default()
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        let mut start = 0.0;
        command.blocks.retain_mut(|(_, block)| {
            if block.sleep {
                start += block.duration;
                return false;
            }
            block.start = start;
            start += block.duration;
            expected_block_has_effect(block)
        });
        command.blocks.extend(immediate);
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

fn block_easing_matches(block: &ExpectedBlock, duration: f32, easing: Option<&str>) -> bool {
    (block.duration == 0.0 && duration == 0.0) || block.easing == easing
}

fn block_matches(expected: &ExpectedBlock, actual: &SongLuaOverlayCommandBlock) -> bool {
    (expected.start - actual.start).abs() <= EPSILON
        && (expected.duration - actual.duration).abs() <= EPSILON
        && block_easing_matches(expected, actual.duration, actual.easing.as_deref())
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
            && block_easing_matches(block, write.duration_seconds, write.easing.as_deref())
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
                    && block_easing_matches(block, write.duration_seconds, write.easing.as_deref())
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

fn compare_commands(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    primary_index: usize,
    parity: &mut Parity,
) {
    parity.section("message commands");
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
            parity.check(true, String::new);
            continue;
        }
        // A receiver may depend on callback state and therefore have runtime
        // captures alongside other actors' static commands for this message.
        // Try those recorded writes before declaring a static mismatch.
        missing.entry(expected.message.clone()).or_default().push((
            label,
            expected.target.clone(),
            expected,
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
                parity.check(true, String::new);
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
            let mut reported = false;
            for _ in &unmatched {
                parity.check_once(false, &mut reported, || {
                    format!(
                        "stateful {message}MessageCommand differs: DeadSync captured {captured} actors, but {} ITGmania targets/properties did not match ({})",
                        unmatched.len(),
                        unmatched.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
                    )
                });
            }
        } else {
            for target in &unmatched {
                parity.check(false, || {
                    format!("missing {message}MessageCommand effects on {target}")
                });
            }
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
    let mut range = (default, default);
    for layer in compiled {
        let state = layer.player_actors[usize::from(player - 1)].initial_state;
        let value = match target {
            SongLuaEaseTarget::PlayerX => state.x,
            SongLuaEaseTarget::PlayerY => state.y,
            SongLuaEaseTarget::PlayerZ => state.z,
            SongLuaEaseTarget::PlayerRotationX => state.rot_x_deg,
            SongLuaEaseTarget::PlayerRotationY => state.rot_y_deg,
            SongLuaEaseTarget::PlayerRotationZ => state.rot_z_deg,
            SongLuaEaseTarget::PlayerSkewX => state.skew_x,
            SongLuaEaseTarget::PlayerSkewY => state.skew_y,
            // Actor::GetZoom is the X scale; uniform writes also emit axis
            // tracks in DeadSync's sampled player transform representation.
            SongLuaEaseTarget::PlayerZoom => state.zoom_x,
            SongLuaEaseTarget::PlayerZoomX => state.zoom_x,
            SongLuaEaseTarget::PlayerZoomY => state.zoom_y,
            SongLuaEaseTarget::PlayerZoomZ => state.zoom_z,
            _ => default,
        };
        range = (range.0.min(value), range.1.max(value));
    }
    for ease in compiled
        .iter()
        .flat_map(|layer| &layer.eases)
        .filter(|ease| {
            (ease.target == *target
                || (*target == SongLuaEaseTarget::PlayerZoom
                    && ease.target == SongLuaEaseTarget::PlayerZoomX))
                && (ease.player.is_none() || ease.player == Some(player))
        })
    {
        for step in 0..=256 {
            let factor = deadsync_gameplay::song_lua_ease_factor(
                ease.easing.as_deref(),
                step as f32 / 256.0,
                ease.opt1,
                ease.opt2,
            );
            let value = (ease.to - ease.from).mul_add(factor, ease.from);
            range = (range.0.min(value), range.1.max(value));
        }
    }
    range
}

fn range_covers(actual: (f32, f32), expected: (f32, f32)) -> bool {
    actual.0 <= expected.0 + 0.03 && actual.1 >= expected.1 - 0.03
}

fn compare_player_operation_ranges(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    parity: &mut Parity,
) {
    parity.section("player ranges");
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
        // Setter ranges do not include uniform zoom writes in the individual
        // axes, and may contain destinations that never render. Prefer the
        // captured current transforms when the oracle provides them.
        let field = match target {
            SongLuaEaseTarget::PlayerX => 1,
            SongLuaEaseTarget::PlayerY => 2,
            SongLuaEaseTarget::PlayerZ => 3,
            SongLuaEaseTarget::PlayerRotationX => 4,
            SongLuaEaseTarget::PlayerRotationZ => 5,
            SongLuaEaseTarget::PlayerRotationY => 6,
            SongLuaEaseTarget::PlayerZoom | SongLuaEaseTarget::PlayerZoomX => 7,
            SongLuaEaseTarget::PlayerZoomY => 8,
            SongLuaEaseTarget::PlayerZoomZ => 9,
            SongLuaEaseTarget::PlayerSkewX => 10,
            SongLuaEaseTarget::PlayerSkewY => 11,
            _ => continue,
        };
        let (native_min, native_max) = trace
            .player_render_tracks
            .iter()
            .find(|track| track.player == usize::from(player))
            .into_iter()
            .flat_map(|track| &track.transform_samples)
            .filter_map(|sample| sample[field])
            .filter(|value| value.is_finite())
            .map(|value| (value, value))
            .reduce(|(low, high), (value, _)| (low.min(value), high.max(value)))
            .unwrap_or((native_min, native_max));
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
        parity.check(covered, || {
            format!(
                "P{player} {} range differs: ITGmania [{native_min:.3}, {native_max:.3}], DeadSync [{:.3}, {:.3}]",
                track.operation,
                compiled_range.0,
                compiled_range.1
            )
        });
    }
}

fn compiled_command_state_at(
    context: &SongLuaCompileContext,
    compiled: &CompiledSongLua,
    overlay_index: usize,
    beat: f32,
    seconds: f32,
) -> SongLuaOverlayState {
    let overlay = &compiled.overlays[overlay_index];
    if overlay.message_commands.is_empty() {
        return overlay.initial_state;
    }
    let mut current = overlay.initial_state;
    let mut active = None::<(&[SongLuaOverlayCommandBlock], SongLuaOverlayState, f32, f32)>;
    for event in compiled.messages.iter().filter(|event| event.beat <= beat) {
        let Some(command) = overlay
            .message_commands
            .iter()
            .find(|command| command.message == event.message)
        else {
            continue;
        };
        let event_seconds = song_elapsed_seconds_at(event.beat, context);
        if let Some((blocks, base, start_seconds, advance)) = active.take() {
            current =
                overlay_state_after_blocks(base, blocks, event_seconds - start_seconds + advance);
        }
        let base = current;
        current = overlay_state_after_blocks(base, &command.blocks, command.frame_advance);
        active = Some((&command.blocks, base, event_seconds, command.frame_advance));
    }
    if let Some((blocks, base, start_seconds, advance)) = active {
        current = overlay_state_after_blocks(base, blocks, seconds - start_seconds + advance);
    }
    current
}

fn apply_compiled_ease(
    context: &SongLuaCompileContext,
    ease: &deadsync_song_lua::SongLuaOverlayEase,
    seconds: f32,
    current: &mut SongLuaOverlayState,
) {
    let end = match ease.span_mode {
        SongLuaSpanMode::Len => ease.start + ease.limit,
        SongLuaSpanMode::End => ease.limit,
    };
    let (start, end) = match ease.unit {
        SongLuaTimeUnit::Beat => (
            song_elapsed_seconds_at(ease.start, context),
            song_elapsed_seconds_at(end, context),
        ),
        SongLuaTimeUnit::Second => (ease.start, end),
        SongLuaTimeUnit::BeatClock => {
            let rate = deadsync_song_lua::song_music_rate(context);
            (ease.start / rate, end / rate)
        }
    };
    if seconds < start {
        return;
    }
    if seconds >= end {
        deadsync_song_lua::apply_overlay_delta(current, &ease.to);
    } else {
        let factor = deadsync_gameplay::song_lua_ease_factor(
            ease.easing.as_deref(),
            (seconds - start) / (end - start),
            ease.opt1,
            ease.opt2,
        );
        deadsync_song_lua::apply_overlay_delta(current, &ease.from);
        deadsync_song_lua::overlay_state_lerp(current, &ease.to, factor);
    }
}

fn compiled_message_state_at(
    context: &SongLuaCompileContext,
    compiled: &CompiledSongLua,
    overlay_index: usize,
    beat: f32,
    seconds: f32,
) -> SongLuaOverlayState {
    let mut current = compiled_command_state_at(context, compiled, overlay_index, beat, seconds);
    for ease in compiled
        .overlay_eases
        .iter()
        .filter(|ease| ease.overlay_index == overlay_index)
    {
        apply_compiled_ease(context, ease, seconds, &mut current);
    }
    current
}

fn compare_column_splines(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    parity.section("column splines");
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: context.song_timing_bpms.clone(),
            ..Default::default()
        },
        &[],
    );
    for player in 0..2 {
        let spline_tracks = compiled
            .iter()
            .flat_map(|layer| {
                deadsync_song_lua::gameplay::song_lua_column_spline_tracks(
                    layer, player, &timing, 0.0,
                )
            })
            .collect::<Vec<_>>();
        let windows = compiled
            .iter()
            .flat_map(|layer| {
                deadsync_song_lua::gameplay::build_song_lua_column_offset_windows_for_player(
                    layer, &timing, player, 0.0,
                )
            })
            .collect::<Vec<_>>();
        let prefix = format!("ScreenGameplay/PlayerP{}/NoteField/Column", player + 1);
        let mut writes = Vec::new();
        for actor in &trace.external_actors {
            let Some((column, handler)) = actor
                .path
                .strip_prefix(&prefix)
                .and_then(|path| path.split_once('/'))
            else {
                continue;
            };
            let rotation = match handler {
                "GetPosHandler"
                | "GetPosHandler/GetSpline"
                | "get_pos_handler"
                | "get_pos_handler/get_spline" => false,
                "GetRotHandler"
                | "GetRotHandler/GetSpline"
                | "get_rot_handler"
                | "get_rot_handler/get_spline" => true,
                _ => continue,
            };
            let Some(column) = column
                .parse::<usize>()
                .ok()
                .and_then(|column| column.checked_sub(1))
            else {
                continue;
            };
            for track in trace
                .operation_tracks
                .iter()
                .filter(|track| track.actor == actor.id)
            {
                for (sequence, beat, seconds, args) in &track.samples {
                    let expected = match track.operation.as_str() {
                        "Spline.SetSplineMode" | "Spline.set_spline_mode"
                            if args.first().and_then(Value::as_str)
                                == Some("NoteColumnSplineMode_Disabled") =>
                        {
                            Some(0.0)
                        }
                        "Spline.SetPoint" | "Spline.set_point"
                            if value_f32(args.first()) == Some(1.0) =>
                        {
                            args.get(1).and_then(|point| {
                                value_f32(point.get(if rotation { 2 } else { 1 }))
                            })
                        }
                        _ => None,
                    };
                    if let Some(expected) = expected {
                        writes.push((column, rotation, *seconds, *sequence, *beat, expected));
                    }
                }
            }
        }
        writes.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
                .then(a.3.cmp(&b.3))
        });
        let mut reported = HashMap::<(usize, bool), bool>::new();
        for (index, &(column, rotation, seconds, _, beat, expected)) in writes.iter().enumerate() {
            // Compare state after the update. A later Disable can override a
            // SetPoint in the same frame; both cannot equal the rendered value.
            if writes
                .get(index + 1)
                .is_some_and(|next| next.0 == column && next.1 == rotation && next.2 == seconds)
            {
                continue;
            }
            let (transforms, splines) =
                deadsync_gameplay::song_lua_column_transforms(&windows, column + 1, seconds);
            let mut actual = if rotation {
                transforms[3]
                    .get(column)
                    .copied()
                    .unwrap_or(0.0)
                    .to_radians()
            } else {
                transforms[1].get(column).copied().unwrap_or(0.0)
                    + splines
                        .get(column)
                        .map_or(0.0, |spline| spline.receptor(beat)[1])
            };
            for track in spline_tracks
                .iter()
                .filter(|track| !rotation && track.column == column)
            {
                if let Some(frame) = track.at_second(seconds) {
                    if let Some(position) = &frame.position {
                        actual = position.coefficients[0][1][0];
                    }
                }
            }
            parity.check_once(
                actual.is_finite() && (actual - expected).abs() <= if rotation { EPSILON } else { 0.03 },
                reported.entry((column, rotation)).or_default(),
                || format!("P{} column {} final spline {} differs at beat {beat:.3}: ITGmania {expected:.3}, DeadSync {actual:.3}", player + 1, column + 1, if rotation { "rotation" } else { "y" }),
            );
        }
    }
}

fn apply_runtime_updates(
    context: &SongLuaCompileContext,
    compiled: &CompiledSongLua,
    overlay_index: usize,
    beat: f32,
    seconds: f32,
    state: &mut SongLuaOverlayState,
) {
    for track in compiled
        .overlay_updates
        .iter()
        .filter(|track| track.overlay_index == overlay_index)
    {
        if let Some(value) = compiled_update_value_at(
            context,
            compiled,
            overlay_index,
            track.target,
            beat,
            seconds,
        ) {
            deadsync_song_lua::playback::apply_overlay_update(state, track.target, &value);
        }
    }
}

fn projected_drawable_map(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
) -> HashMap<String, (usize, usize)> {
    let definitions = trace
        .actor_definitions
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect::<HashMap<_, _>>();
    let mut drawable_map = HashMap::new();
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
            let named = |definition: &NativeInstance, index: usize| {
                definition.name.is_some()
                    && compiled.overlays[index].name.as_deref() == definition.name
                    && kind_name(&compiled.overlays[index].kind) == definition.class
            };
            drawable_map.extend(native.into_iter().zip(deadsync).map(|(definition, index)| {
                // Unordered hidden resource declarations retain their
                // unique name identity. Repeated names keep draw order.
                let mut matches = compiled
                    .overlays
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| named(&definition, *index));
                let index = match (matches.next(), matches.next()) {
                    (Some((unique, _)), None) => unique,
                    _ => index,
                };
                (definition.id.to_owned(), (layer, index))
            }));
        } else {
            // A noteskin placeholder can change drawable kinds without changing
            // the authored tree. Keep checking the other actors in that tree.
            let mut actors = Vec::new();
            collect_native_overlay_definitions(trace, root, &definitions, &mut actors);
            if actors.len() == compiled.overlays.len()
                && actors
                    .iter()
                    .zip(&compiled.overlays)
                    .all(|(native, actual)| native.name == actual.name.as_deref())
            {
                drawable_map.extend(
                    actors
                        .into_iter()
                        .enumerate()
                        .map(|(index, definition)| (definition.id.to_owned(), (layer, index))),
                );
            }
        }
    }
    drawable_map
}

fn compiled_local_states_at(
    compiled: &CompiledSongLua,
    context: &SongLuaCompileContext,
    beat: f32,
    seconds: f32,
) -> Vec<SongLuaOverlayState> {
    let mut local = compiled
        .overlays
        .iter()
        .enumerate()
        .map(|(overlay_index, _)| {
            compiled_command_state_at(context, compiled, overlay_index, beat, seconds)
        })
        .collect::<Vec<_>>();
    // Visit eases once per frame, rather than once per actor per frame.
    for ease in &compiled.overlay_eases {
        apply_compiled_ease(context, ease, seconds, &mut local[ease.overlay_index]);
    }
    // Resolve the last active track per actor/property once. Applying the
    // original track order retains interactions such as size versus stretch,
    // including repeated targets and tracks that have not started yet.
    let time = overlay_update_time(context, compiled.overlay_update_unit, beat, seconds);
    let mut updates = HashMap::with_capacity(compiled.overlay_updates.len());
    for track in compiled.overlay_updates.iter().rev() {
        if track
            .samples
            .first()
            .is_some_and(|sample| sample.time <= time)
        {
            updates
                .entry((track.overlay_index, track.target))
                .or_insert_with(|| {
                    sample_update_track(
                        context,
                        compiled.overlay_update_unit,
                        track.target,
                        &track.samples,
                        beat,
                        seconds,
                    )
                });
        }
    }
    for track in &compiled.overlay_updates {
        if let Some(Some(value)) = updates.get(&(track.overlay_index, track.target))
            && let Some(state) = local.get_mut(track.overlay_index)
        {
            deadsync_song_lua::playback::apply_overlay_update(state, track.target, value);
        }
    }
    local
}

fn compiled_overlay_states_at(
    compiled: &CompiledSongLua,
    context: &SongLuaCompileContext,
    beat: f32,
    seconds: f32,
) -> Vec<SongLuaOverlayState> {
    let local = compiled_local_states_at(compiled, context, beat, seconds);
    let mut states = compose_overlay_states(
        &compiled.overlays,
        &local,
        [compiled.screen_width, compiled.screen_height],
        [seconds, beat],
    );
    let foreground = &compiled.song_foreground;
    let message_seconds = compiled
        .messages
        .iter()
        .map(|event| Some(song_elapsed_seconds_at(event.beat, context)))
        .collect::<Vec<_>>();
    let events = deadsync_song_lua::gameplay::build_song_lua_actor_message_events_for_commands(
        &compiled.messages,
        &message_seconds,
        &foreground.message_commands,
    );
    let layer = deadsync_song_lua::playback::replay_song_lua_message_state(
        seconds,
        foreground.initial_state,
        &foreground.message_commands,
        Some(&events),
    );
    if layer != SongLuaOverlayState::default() {
        for state in &mut states {
            *state = deadsync_song_lua::playback::song_lua_overlay_compose_state::<
                deadsync_assets::noteskin::SpriteSlot,
            >(
                &SongLuaOverlayKind::ActorFrame,
                layer,
                *state,
                compiled.screen_width,
                compiled.screen_height,
                [seconds, beat],
            );
        }
    }
    states
}

#[test]
fn frame_updates_keep_track_precedence() {
    use SongLuaOverlayUpdateTarget as Target;
    use SongLuaOverlayUpdateValue as Update;
    use deadsync_song_lua::{
        SongLuaOverlayActor, SongLuaOverlayUpdateSample, SongLuaOverlayUpdateTrack,
    };
    let track = |overlay_index, target, values: &[(f32, Update)]| SongLuaOverlayUpdateTrack {
        overlay_index,
        target,
        samples: values
            .iter()
            .map(|(beat, value)| SongLuaOverlayUpdateSample {
                time: *beat,
                value: value.clone(),
            })
            .collect(),
    };
    let compiled = CompiledSongLua {
        overlays: (0..2)
            .map(|_| SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            })
            .collect(),
        overlay_updates: vec![
            track(
                0,
                Target::X,
                &[(0.0, Update::F32(0.0)), (2.0, Update::F32(20.0))],
            ),
            track(
                1,
                Target::X,
                &[(0.0, Update::F32(100.0)), (2.0, Update::F32(200.0))],
            ),
            track(
                0,
                Target::Visible,
                &[(0.0, Update::Bool(true)), (1.0, Update::Bool(false))],
            ),
            track(
                0,
                Target::StretchRect,
                &[(0.0, Update::Vec4([0.0, 0.0, 64.0, 64.0]))],
            ),
            track(0, Target::Size, &[(0.0, Update::Vec2([8.0, 16.0]))]),
            track(
                0,
                Target::StretchRect,
                &[(1.0, Update::Vec4([5.0, 6.0, 25.0, 46.0]))],
            ),
            track(
                0,
                Target::X,
                &[(1.0, Update::F32(500.0)), (2.0, Update::F32(600.0))],
            ),
            track(0, Target::X, &[]),
            track(9, Target::X, &[(0.0, Update::F32(42.0))]),
        ],
        ..CompiledSongLua::default()
    };
    let mut context = SongLuaCompileContext::new("", "Track precedence");
    context.song_timing_bpms = vec![(0.0, 60.0), (1.0, 120.0)];
    for beat in [-0.1, 0.0, 0.5, 0.999, 1.0, 1.001, 1.5, 2.0, 3.0] {
        let mut expected = compiled
            .overlays
            .iter()
            .map(|actor| actor.initial_state)
            .collect::<Vec<_>>();
        for (index, state) in expected.iter_mut().enumerate() {
            apply_runtime_updates(
                &context,
                &compiled,
                index,
                beat,
                song_elapsed_seconds_at(beat, &context),
                state,
            );
        }
        let actual = compiled_local_states_at(
            &compiled,
            &context,
            beat,
            song_elapsed_seconds_at(beat, &context),
        );
        assert_eq!(actual, expected, "all fields at beat {beat}");
        if beat == 1.5 {
            assert_eq!(actual[0].x, 550.0);
            assert!((actual[1].x - 183.33333).abs() < 0.0001);
            assert!(!actual[0].visible);
        }
    }
}

#[test]
#[ignore = "benchmarks frame sampling against the per-actor sampler"]
fn frame_track_sampling_benchmark() {
    use SongLuaOverlayUpdateTarget as Target;
    use deadsync_song_lua::{
        SongLuaOverlayActor, SongLuaOverlayUpdateSample, SongLuaOverlayUpdateTrack,
    };
    let compiled = CompiledSongLua {
        overlays: (0..2_000)
            .map(|_| SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            })
            .collect(),
        overlay_updates: (0..2_000)
            .flat_map(|overlay_index| {
                [
                    Target::X,
                    Target::Y,
                    Target::Z,
                    Target::ZoomX,
                    Target::RotationX,
                    Target::RotationY,
                    Target::RotationZ,
                    Target::EffectPeriod,
                ]
                .map(|target| SongLuaOverlayUpdateTrack {
                    overlay_index,
                    target,
                    samples: [0.0, 2.0]
                        .map(|beat| SongLuaOverlayUpdateSample {
                            time: beat,
                            value: SongLuaOverlayUpdateValue::F32(beat + overlay_index as f32),
                        })
                        .into(),
                })
            })
            .collect(),
        ..CompiledSongLua::default()
    };
    let mut context = SongLuaCompileContext::new("", "Large field sampling");
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let mut expected = compiled
        .overlays
        .iter()
        .map(|actor| actor.initial_state)
        .collect::<Vec<_>>();
    let start = std::time::Instant::now();
    for (index, state) in expected.iter_mut().enumerate() {
        apply_runtime_updates(&context, &compiled, index, 1.0, 1.0, state);
    }
    let per_actor = start.elapsed();
    let start = std::time::Instant::now();
    let actual = compiled_local_states_at(&compiled, &context, 1.0, 1.0);
    let per_frame = start.elapsed();
    assert_eq!(
        actual, expected,
        "all 2,000 actors must retain identical state"
    );
    eprintln!(
        "Frame sampling: actors=2000 tracks=16000 per_actor={per_actor:?} per_frame={per_frame:?}"
    );
}

fn native_screen_vertices(sample: &[Value]) -> Option<Vec<[f32; 2]>> {
    sample
        .get(6)?
        .as_array()?
        .iter()
        .map(|vertex| {
            let vertex = vertex.as_array()?;
            Some([value_f32(vertex.first())?, value_f32(vertex.get(1))?])
        })
        .collect()
}

fn native_draw_crop(sample: &[Value]) -> Option<[f32; 4]> {
    let crop = sample.get(11)?.as_array()?;
    Some([
        value_f32(crop.first())?,
        value_f32(crop.get(1))?,
        value_f32(crop.get(2))?,
        value_f32(crop.get(3))?,
    ])
}

fn native_draw_shadow(sample: &[Value]) -> Option<[f32; 6]> {
    let shadow = sample.get(12)?.as_array()?;
    Some([
        value_f32(shadow.first())?,
        value_f32(shadow.get(1))?,
        value_f32(shadow.get(2))?,
        value_f32(shadow.get(3))?,
        value_f32(shadow.get(4))?,
        value_f32(shadow.get(5))?,
    ])
}

fn compiled_world_vertices(state: SongLuaOverlayState, texture_size: [f32; 2]) -> [[f32; 4]; 4] {
    use deadsync_song_lua::playback::actor_conformance as actor;
    let size = state.size.unwrap_or(texture_size);
    let matrix = actor::overlay_sprite_matrix(state, size);
    [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]]
        .map(|[x, y]| actor::project_world(matrix, [x * size[0], y * size[1], 0.0, 1.0]))
}

#[test]
fn replay_keeps_final_callback() {
    crate::paths::init();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&dir, "Final Callback");
    context.song_timing_bpms = vec![
        (0.0, 137.0),
        (112.0, 210.0),
        (168.0, 220.0),
        (182.0, 230.0),
        (189.0, 240.0),
        (196.0, 250.0),
        (210.0, 260.0),
        (218.0, 137.0),
        (346.0, 200.0),
        (349.0, 240.0),
        (363.0, 250.0),
        (377.0, 270.0),
        (384.0, 285.0),
        (391.0, 300.0),
        (398.0, 315.0),
        (405.0, 67.8),
    ];
    context.music_length_seconds = 153.427_175_168_180_2_f64 as f32;
    let compile = |context: &SongLuaCompileContext| {
        deadsync_assets::song_lua::compile_song_lua(&dir.join("final-callback.lua"), context)
            .expect("compile final queued callback")
    };
    let compiled = compile(&context);
    let fade = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Fade"))
        .expect("fixture has its fader");
    assert_eq!(
        compiled_local_states_at(&compiled, &context, 411.988_13, 153.416_67)[fade].diffuse[3],
        0.0
    );
    // Native Actor::Update consumes the clipped final float delta after the
    // earlier sibling starts this two-second tween on beat 412.
    assert_eq!(
        compiled_local_states_at(&compiled, &context, 412.0, context.music_length_seconds)[fade]
            .diffuse[3],
        0.005_249_023_4
    );
    context.music_length_seconds = 153.416_67;
    let clipped = compile(&context);
    assert_eq!(
        compiled_local_states_at(&clipped, &context, 412.0, context.music_length_seconds)[fade]
            .diffuse[3],
        0.0
    );
}

#[test]
fn song_position_keeps_strict_beat_boundaries() {
    crate::paths::init();
    let song_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&song_dir, "Beat Boundary");
    context.song_timing_bpms = vec![(0.0, 200.0)];
    context.music_length_seconds = 18.7;
    let compiled =
        deadsync_assets::song_lua::compile_song_lua(&song_dir.join("beat-boundary.lua"), &context)
            .unwrap();
    for (name, expected) in [("Strict", 0.0), ("Inclusive", 1.0), ("Position", 0.0)] {
        let index = compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some(name))
            .unwrap();
        let at_boundary = compiled_local_states_at(&compiled, &context, 62.0, 18.6)[index].x;
        assert_eq!(at_boundary, expected, "{name} at beat 62");
        assert_eq!(
            compiled_local_states_at(&compiled, &context, 62.05556, 18.616667)[index].x,
            1.0,
            "{name} on the next frame"
        );
    }
}

#[test]
fn runtime_size_zoom_matches_native_drawing() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&song_dir, "Runtime Size Zoom");
    context.screen_width = 854.0;
    context.music_length_seconds = 1.5;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(
        &[song_dir.join("runtime-size-zoom.lua").as_path()],
        0,
        &context,
    )
    .expect("compile runtime size fixture");
    let native: Value = serde_json::from_slice(
        &fs::read(
            root.join("tests/fixtures/itgmania-song-lua-micro/runtime-size-zoom-native.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut checks = 0;
    for sample in native["samples"].as_array().unwrap() {
        let second = sample["time"].as_f64().unwrap() as f32;
        let states = compiled_overlay_states_at(&compiled[0], &context, second, second);
        for actor in sample["actors"].as_array().unwrap().iter().skip(1) {
            let name = actor["name"].as_str().unwrap();
            let index = compiled[0]
                .overlays
                .iter()
                .position(|overlay| overlay.name.as_deref() == Some(name))
                .unwrap();
            let state = states[index];
            if name == "Resized" {
                let expected =
                    std::array::from_fn(|axis| actor["size"][axis].as_f64().unwrap() as f32);
                assert_eq!(state.size, Some(expected), "size at {second}");
            }
            let zoom = actor["current"]["zoom"].as_array().unwrap();
            for (axis, actual) in [state.zoom_x, state.zoom_y, state.zoom_z]
                .into_iter()
                .enumerate()
            {
                let expected = zoom[axis].as_f64().unwrap() as f32;
                assert!(
                    (actual - expected).abs() <= 0.002,
                    "{name} zoom {axis} at {second}: {expected} vs {actual}"
                );
                checks += 1;
            }
            let Some(draw) = actor["draws"].as_array().and_then(|draws| draws.first()) else {
                continue;
            };
            let size = if name == "Beat" {
                [64.0, 32.0]
            } else {
                [1280.0, 720.0]
            };
            let vertices = compiled_world_vertices(state, size);
            for (corner, actual) in vertices.iter().enumerate() {
                let vertex = &draw["vertices"][[0, 3, 2, 1][corner]];
                for axis in 0..2 {
                    let expected = vertex["screen"][axis].as_f64().unwrap() as f32;
                    assert!(
                        (expected - actual[axis]).abs() <= 0.75,
                        "{name} corner {corner} at {second}: {expected} vs {actual:?}"
                    );
                    checks += 1;
                }
            }
        }
    }
    assert_eq!(checks, 176);
}

#[test]
fn affine_skew_matches_native_drawing() {
    assert_native_actor_drawing("affine-skew");
}

#[test]
fn collapsed_transform_matches_native_drawing() {
    assert_native_actor_drawing("collapsed-transform");
}

#[test]
fn ancestor_scale_matches_native_drawing() {
    assert_native_actor_drawing("ancestor-scale");
}

#[test]
fn parent_rotation_matches_native_drawing() {
    assert_native_actor_drawing("parent-rotation");
}

fn assert_native_actor_drawing(fixture: &str) {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&song_dir, fixture);
    context.screen_width = 854.0;
    let compiled = compile_song_lua_layers(
        &[song_dir.join(format!("{fixture}.lua")).as_path()],
        0,
        &context,
    )
    .expect("compile native drawing fixture");
    let native: Value = serde_json::from_slice(
        &fs::read(root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{fixture}-native.json"
        )))
        .expect("native drawing"),
    )
    .expect("native actor fixture");
    assert_eq!(native["oracle"], "itgmania_native_actor_conformance");
    let states = compiled_overlay_states_at(&compiled[0], &context, 0.0, 0.0);
    let mut checks = 0;
    for actor in native["samples"][0]["actors"]
        .as_array()
        .expect("native actors")
        .iter()
        .filter(|actor| actor["kind"] == "sprite")
    {
        let name = actor["name"].as_str().expect("sprite name");
        let index = compiled[0]
            .overlays
            .iter()
            .position(|actor| actor.name.as_deref() == Some(name))
            .expect("compiled sprite");
        let actual = compiled_world_vertices(states[index], [64.0, 32.0]);
        for (corner, actual) in actual.iter().enumerate() {
            let expected = &actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]]["world"];
            for axis in 0..3 {
                let expected = expected[axis].as_f64().expect("world coordinate") as f32;
                assert!(
                    (actual[axis] - expected).abs() < 0.0001,
                    "{name} corner {corner} axis {axis}: {} vs {expected}",
                    actual[axis]
                );
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 36);
}

#[test]
fn bpm_fade_matches_native_clock() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace = read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/bpm-fade.json"));
    let mut context = SongLuaCompileContext::new(&song_dir, trace.title.clone());
    context.screen_width = 854.0;
    context.music_length_seconds = 1.5;
    context.song_timing_bpms = vec![(0.0, 333.0), (5.0, 666.0)];
    let compiled = compile_song_lua_layers(&[song_dir.join("bpm-fade.lua").as_path()], 0, &context)
        .expect("compile fade spanning the BPM change");
    let parity = compare_semantics(&trace, &compiled, 0, &context);
    eprintln!("{}", parity.summary("BPM fade"));
    assert_eq!(parity.checks(), 120);
    parity.assert_complete("BPM fade");
    let layer = &compiled[0];
    let index = layer
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Fade"))
        .expect("fade sprite");
    let native: Value = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-song-lua-micro/bpm-fade-native.json"))
            .expect("native alpha capture"),
    )
    .expect("valid native alpha capture");
    let mut checks = 0;
    for sample in native["samples"].as_array().expect("native samples") {
        let second = sample["time"].as_f64().expect("native time") as f32;
        let beat = song_beat_at_elapsed_seconds(second, &context);
        let actual = compiled_local_states_at(layer, &context, beat, second)[index].diffuse[3];
        let actor = sample["actors"]
            .as_array()
            .expect("native actors")
            .iter()
            .find(|actor| actor["name"] == "Fade")
            .expect("native fade sprite");
        let expected = actor["current"]["diffuse"][0][3]
            .as_f64()
            .expect("native alpha") as f32;
        assert!(
            (actual - expected).abs() <= 0.0001,
            "fade at second {second}, beat {beat}: {actual} vs native {expected}"
        );
        checks += 1;
    }
    assert_eq!(checks, 9);
}

#[test]
fn waltz_runtime_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/waltz-runtime.json"));
    let mut context = SongLuaCompileContext::new(&song_dir, trace.title.clone());
    context.screen_width = 854.0;
    context.music_length_seconds = 3.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[song_dir.join("waltz-runtime.lua").as_path()], 0, &context)
            .expect("compile Waltz runtime fixture");
    let mut parity = compare_semantics(&trace, &compiled, 0, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Waltz runtime"));
    assert_eq!(parity.checks(), 287);
    parity.assert_complete("Waltz runtime");
    let layer = &compiled[0];
    let screen = layer.screen_overlay_index.expect("captured top screen");
    let before = compiled_local_states_at(layer, &context, 1.5, 1.5)[screen];
    assert!(before.vibrate);
    assert_eq!(before.effect_magnitude, [20.0, 20.0, 0.0]);
    let after = compiled_local_states_at(layer, &context, 2.1, 2.1)[screen];
    assert_eq!(after.effect_magnitude, [0.0; 3]);
    let helper = layer
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Helper"))
        .unwrap();
    for second in [2.0, 2.1, 2.5, 3.0] {
        assert_eq!(
            compiled_local_states_at(layer, &context, second, second)[helper].y,
            0.0
        );
    }
    let native: Value = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-song-lua-micro/waltz-base-zoom-native.json"))
            .unwrap(),
    )
    .unwrap();
    let sprite = layer
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Sprite"))
        .unwrap();
    let mut checks = 0;
    for sample in native["samples"].as_array().unwrap() {
        let second = sample["time"].as_f64().unwrap() as f32;
        let state = compiled_overlay_states_at(layer, &context, second, second)[sprite];
        let actual = compiled_world_vertices(state, [64.0, 32.0]);
        let actor = sample["actors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|actor| actor["name"] == "Sprite")
            .unwrap();
        for corner in 0..4 {
            for axis in 0..2 {
                let expected = actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]]["screen"][axis]
                    .as_f64()
                    .unwrap() as f32;
                assert!(
                    (actual[corner][axis] - expected).abs() <= 0.002,
                    "base zoom at {second}: {actual:?} vs {expected}"
                );
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 56);
}

#[test]
fn fallback_tweens_match_native_drawing() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&dir, "Fallback Tweens");
    context.screen_width = 854.0;
    context.music_length_seconds = 0.5;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[dir.join("fallback-tweens.lua").as_path()], 0, &context).unwrap();
    let native: Value = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-actors/fallback-tweens.json")).unwrap(),
    )
    .unwrap();
    let mut checks = 0;
    for sample in native["samples"].as_array().unwrap() {
        let time = sample["time"].as_f64().unwrap() as f32;
        let states = compiled_overlay_states_at(&compiled[0], &context, time, time);
        for actor in sample["actors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["kind"] == "sprite")
        {
            let index = compiled[0]
                .overlays
                .iter()
                .position(|a| a.name.as_deref() == actor["name"].as_str())
                .unwrap();
            let state = states[index];
            assert_eq!(
                state.visible,
                actor["visible"].as_bool().unwrap(),
                "{} at {time}",
                actor["name"]
            );
            checks += 1;
            if !state.visible {
                continue;
            }
            let vertices = compiled_world_vertices(state, [64.0, 32.0]);
            for corner in 0..4 {
                for axis in 0..2 {
                    let expected = actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]]["screen"]
                        [axis]
                        .as_f64()
                        .unwrap() as f32;
                    let actual = vertices[corner][axis];
                    assert!(
                        (expected - actual).abs() <= 0.002,
                        "{} at {time}: {expected} vs {actual}",
                        actor["name"]
                    );
                    checks += 1;
                }
            }
        }
    }
    assert_eq!(checks, 184);
}

#[test]
fn spin_tweens_match_native_rotation() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&dir, "Spin Tweens");
    context.screen_width = 854.0;
    context.music_length_seconds = 2.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[dir.join("spin-tween.lua").as_path()], 0, &context).unwrap();
    let native: Value = serde_json::from_reader(
        zstd::stream::read::Decoder::new(
            fs::File::open(
                root.join("tests/fixtures/itgmania-song-lua-micro/spin-tween-native.json.zst"),
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut checks = 0;
    for sample in native["samples"].as_array().unwrap() {
        let time = sample["time"].as_f64().unwrap() as f32;
        let states = compiled_overlay_states_at(&compiled[0], &context, time, time);
        for actor in sample["actors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["kind"] == "sprite")
        {
            let index = compiled[0]
                .overlays
                .iter()
                .position(|a| a.name.as_deref() == actor["name"].as_str())
                .unwrap();
            let mut state = states[index];
            assert!(state.spin_baked);
            let rendered =
                deadsync_song_lua::playback::actor_conformance::effect_sample(state, time, time);
            [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = rendered.rotation;
            let expected = actor["current"]["rotation"][2].as_f64().unwrap() as f32;
            assert!(
                (state.rot_z_deg - expected).abs() <= 0.001,
                "{} at {time}: {} vs {expected}",
                actor["name"],
                state.rot_z_deg
            );
            let vertices = compiled_world_vertices(state, [64.0, 32.0]);
            for corner in 0..4 {
                for axis in 0..2 {
                    let expected = actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]]["screen"]
                        [axis]
                        .as_f64()
                        .unwrap() as f32;
                    assert!(
                        (vertices[corner][axis] - expected).abs() <= 0.002,
                        "{} at {time}: {} vs {expected}",
                        actor["name"],
                        vertices[corner][axis]
                    );
                    checks += 1;
                }
            }
        }
    }
    assert_eq!(checks, 1936);
}

#[test]
fn queued_update_matches_native_order() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/song-lua");
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/phase-update.json"));
    let mut context = SongLuaCompileContext::new(&dir, "Phase Update");
    context.screen_width = 854.0;
    context.music_length_seconds = trace.end_position.seconds;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[dir.join("phase-update.lua").as_path()], 0, &context).unwrap();
    let parity = compare_semantics(&trace, &compiled, 0, &context);
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("native queued update order");
}

#[test]
fn zero_queue_keeps_current_position() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/song-lua");
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/message-read-order.json"),
    );
    let mut context = SongLuaCompileContext::new(&dir, &trace.title);
    context.music_length_seconds = trace.end_position.seconds;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[dir.join("message-read-order.lua").as_path()], 0, &context)
            .expect("compile repeated queued reader");
    let mut parity = compare_semantics(&trace, &compiled, 0, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("zero-duration tween current position");
}

#[test]
fn retarget_update_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/song-lua");
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/retarget-update.json"));
    let mut context = SongLuaCompileContext::new(&dir, "Retarget Update");
    context.screen_width = 854.0;
    context.music_length_seconds = trace.end_position.seconds;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[dir.join("retarget-update.lua").as_path()], 0, &context).unwrap();
    let parity = compare_semantics(&trace, &compiled, 0, &context);
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("native overlay destination changes");
}

#[test]
fn queued_commands_match_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/song-lua");
    for name in [
        "queued-effects",
        "queued-fade",
        "queued-chain",
        "callback-phase",
        "finish-queue",
        "late-texture",
        "spin-update",
    ] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json"
        )));
        let mut context = SongLuaCompileContext::new(&dir, &trace.title);
        context.screen_width = 854.0;
        context.music_length_seconds = trace.end_position.seconds;
        context.song_display_bpms = [60.0; 2];
        context.song_timing_bpms = vec![(0.0, 60.0)];
        let compiled =
            compile_song_lua_layers(&[dir.join(format!("{name}.lua")).as_path()], 0, &context)
                .unwrap();
        if name == "spin-update" {
            let index = compiled[0]
                .overlays
                .iter()
                .position(|actor| actor.name.as_deref() == Some("Spinning"))
                .unwrap();
            assert!(
                compiled[0].overlays[index]
                    .message_commands
                    .iter()
                    .all(|command| command.message != "Context")
            );
            let mut incorrect = compiled.clone();
            let write = incorrect[0]
                .stateful_message_captures
                .iter_mut()
                .find(|capture| capture.message == "Context")
                .unwrap()
                .writes
                .iter_mut()
                .find(|write| {
                    write.overlay_index == index && write.target == SongLuaOverlayUpdateTarget::Zoom
                })
                .unwrap();
            write.value = deadsync_song_lua::SongLuaOverlayUpdateValue::F32(999.0);
            let mut rejected = Parity::default();
            compare_commands(&trace, &incorrect, 0, &mut rejected);
            assert!(
                !rejected.gaps.is_empty(),
                "incorrect callback-dependent message must fail"
            );
        }
        let parity = compare_semantics(&trace, &compiled, 0, &context);
        eprintln!("{}", parity.summary(&trace.title));
        parity.assert_complete(name);
    }
}

#[test]
fn random_probe_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/song-lua");
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/random-probe.json"));
    let mut context = SongLuaCompileContext::new(&dir, "Random Probe");
    context.screen_width = 854.0;
    context.music_length_seconds = trace.end_position.seconds;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[dir.join("random-probe.lua").as_path()], 0, &context).unwrap();
    let parity = compare_semantics(&trace, &compiled, 0, &context);
    assert!(
        parity
            .sections
            .iter()
            .map(|section| section.checks)
            .sum::<usize>()
            > 300,
        "native random probe fixture lost its writes"
    );
    parity.assert_complete("native random probe");
}

#[test]
fn additive_update_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/song-lua");
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/additive-update.json"));
    let mut context = SongLuaCompileContext::new(&dir, "Additive Update");
    context.screen_width = 854.0;
    context.music_length_seconds = trace.end_position.seconds;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled =
        compile_song_lua_layers(&[dir.join("additive-update.lua").as_path()], 0, &context).unwrap();
    let definition = trace
        .actor_definitions
        .iter()
        .find(|a| a.name.as_deref() == Some("SetterAdd"))
        .unwrap();
    let writes = native_update_render_writes_all(&trace, &definition.id);
    use SongLuaOverlayUpdateTarget as Target;
    for (target, expected) in [
        (Target::X, 105.0),
        (Target::Y, 160.0),
        (Target::Z, 3.0),
        (Target::RotationX, 12.0),
        (Target::RotationY, 17.0),
        (Target::RotationZ, 34.0),
    ] {
        let write = writes.iter().find(|w| w.target == target).unwrap();
        assert_eq!(
            write.value,
            SongLuaOverlayUpdateValue::F32(expected),
            "{target:?}"
        );
    }
    let parity = compare_semantics(&trace, &compiled, 0, &context);
    assert!(parity.checks() > 500);
    parity.assert_complete("native additive update");
}

#[test]
fn perspective_float_matches_native_drawing() {
    compare_lua_perspective("perspective-float.lua");
}

#[test]
fn perspective_fields_match_native_drawing() {
    compare_lua_perspective("perspective-fields.lua");
}

fn compare_lua_perspective(entry: &str) {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&song_dir, "Perspective Float");
    context.screen_width = 854.0;
    context.music_length_seconds = 83.7;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(&[song_dir.join(entry).as_path()], 0, &context)
        .expect("compile perspective fixture");
    let native: Value = serde_json::from_slice(
        &fs::read(
            root.join("tests/fixtures/itgmania-song-lua-micro/perspective-float-native.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut checks = 0;
    for sample in native["samples"].as_array().unwrap() {
        let second = sample["time"].as_f64().unwrap() as f32;
        let states = compiled_overlay_states_at(&compiled[0], &context, second, second);
        for actor in sample["actors"].as_array().unwrap().iter().filter(|actor| {
            actor["draws"]
                .as_array()
                .is_some_and(|draws| !draws.is_empty())
        }) {
            let index = compiled[0]
                .overlays
                .iter()
                .position(|overlay| overlay.name.as_deref() == actor["name"].as_str())
                .unwrap();
            let mut state = states[index];
            let effect = deadsync_song_lua::playback::actor_conformance::effect_sample(
                state, second, second,
            );
            [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = effect.rotation;
            let vertices = compiled_perspective_vertices(
                &compiled[0],
                &states,
                index,
                state,
                [64.0, 32.0],
                None,
            )
            .expect("finite perspective vertices");
            for (corner, actual) in vertices.iter().enumerate() {
                let vertex = &actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]];
                for axis in 0..2 {
                    let expected = vertex["screen"][axis].as_f64().unwrap() as f32;
                    assert!(
                        (expected - actual[axis]).abs() <= 0.75,
                        "{} corner {corner} axis {axis}: {expected} vs {actual:?}",
                        actor["name"]
                    );
                    checks += 1;
                }
            }
        }
    }
    assert_eq!(checks, 112);
}

fn compiled_perspective_vertices(
    compiled: &CompiledSongLua,
    states: &[SongLuaOverlayState],
    index: usize,
    state: SongLuaOverlayState,
    texture_size: [f32; 2],
    screen_camera: Option<SongLuaOverlayState>,
) -> Option<[[f32; 2]; 4]> {
    use deadsync_song_lua::playback::actor_conformance as actor;
    let mut parent = compiled.overlays[index].parent_index;
    let camera = loop {
        let Some(index) = parent else {
            break screen_camera?;
        };
        let overlay = &compiled.overlays[index];
        if matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture { .. }) {
            return None;
        }
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture { .. }
        ) && states[index].fov.is_some()
        {
            break states[index];
        }
        parent = overlay.parent_index;
    };
    let screen = [compiled.screen_width, compiled.screen_height];
    let mut viewport = screen;
    let mut parent = compiled.overlays[index].parent_index;
    while let Some(index) = parent {
        if let SongLuaOverlayKind::ActorFrameTexture { capture_size, .. } =
            compiled.overlays[index].kind
        {
            if !states[index].aft_created {
                return None;
            }
            viewport = capture_size?;
            break;
        }
        parent = compiled.overlays[index].parent_index;
    }
    let (view, projection) = actor::view_projection(
        screen.map(|axis| axis as u32),
        camera.fov?,
        camera.vanishpoint.unwrap_or(screen.map(|axis| axis * 0.5)),
    );
    let corners = compiled_world_vertices(state, texture_size)
        .map(|world| actor::project_world(projection, actor::project_world(view, world)));
    // The reference records projected corners before GPU clipping, including
    // negative W. Only an undefined perspective divide prevents comparison.
    if corners.iter().any(|corner| {
        corner.iter().any(|axis| !axis.is_finite()) || corner[3].abs() <= f32::EPSILON
    }) {
        return None;
    }
    Some(corners.map(|[x, y, _, w]| {
        let inverse_w = w.recip();
        [
            (x * inverse_w + 1.0) * viewport[0] * 0.5,
            (1.0 - y * inverse_w) * viewport[1] * 0.5,
        ]
    }))
}

fn vertex_bounds(vertices: &[[f32; 2]]) -> [f32; 4] {
    vertices.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |[min_x, min_y, max_x, max_y], [x, y]| {
            [min_x.min(*x), min_y.min(*y), max_x.max(*x), max_y.max(*y)]
        },
    )
}

fn vertex_center(vertices: &[[f32; 2]]) -> [f32; 2] {
    let sum = vertices
        .iter()
        .fold([0.0, 0.0], |[x, y], vertex| [x + vertex[0], y + vertex[1]]);
    let count = vertices.len().max(1) as f32;
    [sum[0] / count, sum[1] / count]
}

#[test]
fn projected_spin_matches_native_draws() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/itgmania-actors/effects-vibration.json");
    let native: Value = serde_json::from_slice(&fs::read(path).expect("native effect fixture"))
        .expect("valid native effect fixture");
    let samples = native["samples"]
        .as_array()
        .expect("native samples")
        .iter()
        .map(|sample| {
            let spin = sample["actors"]
                .as_array()
                .expect("native actors")
                .iter()
                .find(|actor| actor["name"] == "spin")
                .expect("native spin actor");
            let vertices = spin["draws"][0]["vertices"]
                .as_array()
                .expect("native vertices")
                .iter()
                .map(|vertex| serde_json::json!([vertex["screen"][0], vertex["screen"][1]]))
                .collect::<Vec<_>>();
            serde_json::json!([sample["beat"], sample["time"], true, 1.0, [], [], vertices])
        })
        .collect::<Vec<_>>();
    let trace: NativeTrace = serde_json::from_value(serde_json::json!({
        "oracle": "itgmania_native_actor_conformance", "title": "spin", "style": "single",
        "simfile": "", "roots": ["root"], "runtime_actors": [],
        "actor_definitions": [
            {"id": "root", "class": "ActorFrame", "children": [{"layer_index": 1, "definition_id": "spin"}]},
            {"id": "spin", "class": "Quad", "name": "spin"}
        ],
        "timeline_tracks": [], "tween_tracks": [], "end_position": {"seconds": 1.0},
        "display": {"width": 640, "height": 480, "logical_width": 640, "logical_height": 480},
        "fixture_context": {"beat_step": 0.25}, "trace_until_beat": 1.0,
        "projected_vertex_tracks": [{"actor": "spin", "definition_id": "spin", "texture": "",
            "texture_size": [64, 64], "camera_actor": "orthographic-screen", "sample_layout": [], "samples": samples}]
    })).expect("native draw adapter");
    let mut compiled = CompiledSongLua {
        screen_width: 640.0,
        screen_height: 480.0,
        overlays: vec![
            deadsync_song_lua::SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            },
            deadsync_song_lua::SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: Some("spin".to_owned()),
                parent_index: Some(0),
                initial_state: SongLuaOverlayState {
                    x: 380.0,
                    y: 280.0,
                    effect_mode: EffectMode::Spin,
                    effect_magnitude: [30.0, 60.0, 90.0],
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            },
        ],
        ..CompiledSongLua::default()
    };
    let context = SongLuaCompileContext::new("", "spin");
    let mut parity = Parity::default();
    compare_projected_geometry(
        &trace,
        std::slice::from_ref(&compiled),
        &context,
        &mut parity,
    );
    parity.assert_complete("native spin draws");
    assert!(parity.checks() > 4, "exercise nonzero effect times");
    compiled.overlays[1].initial_state.effect_mode = EffectMode::None;
    let mut stationary = Parity::default();
    compare_projected_geometry(&trace, &[compiled], &context, &mut stationary);
    assert!(
        !stationary.gaps.is_empty(),
        "stationary geometry must fail the native spin audit"
    );
}

#[test]
fn projected_corners_keep_negative_w() {
    let camera = SongLuaOverlayState {
        fov: Some(90.0),
        ..SongLuaOverlayState::default()
    };
    let sprite = SongLuaOverlayState {
        x: 320.0,
        y: 240.0,
        z: 321.0,
        ..SongLuaOverlayState::default()
    };
    let compiled = CompiledSongLua {
        screen_width: 640.0,
        screen_height: 480.0,
        overlays: vec![
            deadsync_song_lua::SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: None,
                initial_state: camera,
                message_commands: Vec::new(),
            },
            deadsync_song_lua::SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: None,
                parent_index: Some(0),
                initial_state: sprite,
                message_commands: Vec::new(),
            },
        ],
        ..CompiledSongLua::default()
    };
    let vertices =
        compiled_perspective_vertices(&compiled, &[camera, sprite], 1, sprite, [64.0; 2], None)
            .expect("negative W is a defined perspective divide");
    assert!(vertices.iter().flatten().all(|axis| axis.is_finite()));
    assert!(
        vertex_center(&vertices)
            .iter()
            .zip([320.0, 240.0])
            .all(|(actual, expected)| (actual - expected).abs() <= 0.01)
    );
    let singular = SongLuaOverlayState { z: 320.0, ..sprite };
    assert!(
        compiled_perspective_vertices(&compiled, &[camera, singular], 1, singular, [64.0; 2], None)
            .is_none()
    );
}

#[test]
fn projected_quad_uses_unit_geometry() {
    let trace: NativeTrace = serde_json::from_value(serde_json::json!({
        "oracle": "itgmania_native_actor_conformance", "title": "quad size", "style": "single",
        "simfile": "", "roots": ["root"], "runtime_actors": [],
        "actor_definitions": [
            {"id": "root", "class": "ActorFrame", "children": [{"layer_index": 1, "definition_id": "quad"}]},
            {"id": "quad", "class": "Quad", "name": "quad"}
        ],
        "timeline_tracks": [], "tween_tracks": [], "end_position": {"seconds": 1.0},
        "display": {"width": 854, "height": 480, "logical_width": 854, "logical_height": 480},
        "fixture_context": {"beat_step": 0.25}, "trace_until_beat": 1.0,
        "projected_vertex_tracks": [{"actor": "quad", "definition_id": "quad", "texture": "",
            "texture_size": [854, 480], "camera_actor": "orthographic-screen", "sample_layout": [],
            "samples": [[0, 0, true, 1, [], [], [[0, 0], [854, 0], [854, 480], [0, 480]]]]}]
    })).expect("native zoomed quad reference");
    let mut compiled = CompiledSongLua {
        screen_width: 854.0,
        screen_height: 480.0,
        overlays: vec![deadsync_song_lua::SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: Some("quad".into()),
            parent_index: None,
            initial_state: SongLuaOverlayState {
                x: 427.0,
                y: 240.0,
                zoom_x: 854.0,
                zoom_y: 480.0,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        }],
        ..CompiledSongLua::default()
    };
    let context = SongLuaCompileContext::new("", "quad size");
    let mut parity = Parity::default();
    compare_projected_geometry(
        &trace,
        std::slice::from_ref(&compiled),
        &context,
        &mut parity,
    );
    assert_eq!(parity.checks(), 4);
    parity.assert_complete("zoomed quad");
    compiled.overlays[0].initial_state.zoom_x *= 2.0;
    let mut regression = Parity::default();
    compare_projected_geometry(&trace, &[compiled], &context, &mut regression);
    assert!(
        regression
            .gaps
            .iter()
            .any(|gap| gap.contains("projected bounds differ"))
    );
}

#[test]
fn base_rotation_keeps_getters_and_draw_pose() {
    crate::paths::init();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&dir, "Base rotation");
    context.song_timing_bpms = vec![(0.0, 60.0)];
    context.song_display_bpms = [60.0; 2];
    context.music_length_seconds = 2.0;
    let compiled = compile_song_lua_layers(&[dir.join("base-rotation.lua").as_path()], 0, &context)
        .expect("base rotation compile");
    let layer = &compiled[0];
    assert!(
        layer
            .messages
            .iter()
            .any(|event| event.message == "10:15:5"),
        "getters exclude base rotation"
    );
    let index = layer
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("BaseRotation"))
        .expect("quad actor");
    assert!(
        layer
            .overlay_updates
            .iter()
            .any(|track| track.overlay_index == index
                && track.target == SongLuaOverlayUpdateTarget::BaseRotation),
        "capture dynamic base rotation"
    );
    for (beat, base, rotation) in [(0.5, 90.0, 95.0), (1.5, 45.0, 60.0)] {
        let local = compiled_local_states_at(layer, &context, beat, beat)[index];
        assert_eq!(local.base_rotation, [-60.0, 20.0, base]);
        assert!((local.rot_z_deg - beat * 10.0).abs() < 0.001);
        let drawn =
            deadsync_song_lua::playback::actor_conformance::transform_state(local, [beat, beat]);
        assert_eq!([drawn.rot_x_deg, drawn.rot_y_deg], [-50.0, 35.0]);
        assert!((drawn.rot_z_deg - rotation).abs() < 0.001);
        assert_eq!(
            drawn.base_rotation, [0.0; 3],
            "composition consumes base rotation once"
        );
    }
}

#[test]
fn projected_colors_keep_nonfinite_kind() {
    for (kind, value) in [
        ("infinity", f32::INFINITY),
        ("-infinity", f32::NEG_INFINITY),
        ("nan", f32::NAN),
    ] {
        let trace: NativeTrace = serde_json::from_value(serde_json::json!({
            "oracle": "itgmania_native_actor_conformance", "title": "nonfinite color", "style": "single",
            "simfile": "", "roots": ["root"], "runtime_actors": [],
            "actor_definitions": [
                {"id": "root", "class": "ActorFrame", "children": [{"layer_index": 1, "definition_id": "quad"}]},
                {"id": "quad", "class": "Quad", "name": "quad"}
            ],
            "timeline_tracks": [], "tween_tracks": [], "end_position": {"seconds": 1.0},
            "display": {"width": 640, "height": 480, "logical_width": 640, "logical_height": 480},
            "fixture_context": {"beat_step": 0.25}, "trace_until_beat": 1.0,
            "projected_vertex_tracks": [{"actor": "quad", "definition_id": "quad", "texture": "",
                "texture_size": [1, 1], "camera_actor": "orthographic-screen", "sample_layout": [],
                "samples": [[0, 0, true, 1, [], [], [], [], [],
                    [{"type": "number", "value": kind}, 1, 1, 1], [1, 1, 1, 0]]]}]
        })).expect("native nonfinite color reference");
        let mut compiled = CompiledSongLua {
            overlays: vec![deadsync_song_lua::SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: Some("quad".into()),
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    diffuse: [value, 1.0, 1.0, 1.0],
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            }],
            ..CompiledSongLua::default()
        };
        let context = SongLuaCompileContext::new("", "nonfinite color");
        let mut parity = Parity::default();
        compare_projected_geometry(
            &trace,
            std::slice::from_ref(&compiled),
            &context,
            &mut parity,
        );
        assert_eq!(parity.checks(), 10, "keep every color and visibility check");
        parity.assert_complete(kind);
        compiled.overlays[0].initial_state.diffuse[0] = if value.is_nan() { 1.0 } else { -value };
        let mut regression = Parity::default();
        compare_projected_geometry(&trace, &[compiled], &context, &mut regression);
        assert!(
            regression
                .gaps
                .iter()
                .any(|gap| gap.contains("draw color differs")),
            "the wrong nonfinite sign or class must fail"
        );
    }
}

fn compare_projected_geometry(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    let screen_layer = compiled
        .iter()
        .find_map(|layer| layer.screen_overlay_index.map(|index| (layer, index)));
    let mut screen_states = HashMap::new();
    parity.section("projected geometry");
    let drawable_map = projected_drawable_map(trace, compiled);
    let mut colors = Parity::default();
    colors.section("draw colors");
    let mut crops = Parity::default();
    crops.section("draw crops");
    let mut shadows = Parity::default();
    shadows.section("draw shadows");
    let mut state_cache = HashMap::<(usize, u32, u32), Vec<SongLuaOverlayState>>::new();
    for track in &trace.projected_vertex_tracks {
        let Some(definition_id) = track.definition_id.as_deref() else {
            continue;
        };
        let Some(&(layer, overlay_index)) = drawable_map.get(&track.actor) else {
            parity.check(false, || {
                format!(
                    "projected geometry is untested for {definition_id}: no matching DeadSync actor"
                )
            });
            continue;
        };
        let mut reported_colors = [false; 8];
        let mut reported_crops = [false; 4];
        let mut reported_shadows = [false; 6];
        let mut reported_visibility = false;
        let mut reported_alpha = false;
        let mut reported_nonfinite = false;
        let mut reported_bounds = false;
        let mut reported_center = false;
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
            let Some(native_visible) = sample.get(2).and_then(Value::as_bool) else {
                continue;
            };
            let Some(native_alpha) = sample.get(3).and_then(projected_alpha) else {
                parity.check_once(false, &mut reported_nonfinite, || {
                    format!("reference projected alpha is non-finite for {definition_id} at beat {beat:.3}")
                });
                continue;
            };
            let states = state_cache
                .entry((layer, beat.to_bits(), seconds.to_bits()))
                .or_insert_with(|| {
                    let mut states =
                        compiled_overlay_states_at(&compiled[layer], context, beat, seconds);
                    if let Some((screen_layer, index)) = screen_layer {
                        let screen = screen_states
                            .entry((beat.to_bits(), seconds.to_bits()))
                            .or_insert_with(|| {
                                let mut screen = compiled_command_state_at(
                                    context,
                                    screen_layer,
                                    index,
                                    beat,
                                    seconds,
                                );
                                apply_runtime_updates(
                                    context,
                                    screen_layer,
                                    index,
                                    beat,
                                    seconds,
                                    &mut screen,
                                );
                                let screen =
                                    deadsync_song_lua::playback::actor_conformance::transform_state(
                                        screen,
                                        [seconds, beat],
                                    );
                                screen
                            });
                        // GameplayActorSegments::segments places every screen
                        // fragment at this shared offset before its camera.
                        for state in &mut states {
                            state.x += screen.x;
                            state.y += screen.y;
                        }
                    }
                    states
                });
            let Some(mut state) = states.get(overlay_index).copied() else {
                continue;
            };
            // Gameplay applies spin after composing the actor's local state.
            // Compare the rendered rotation, rather than its stationary base.
            if state.effect_mode == EffectMode::Spin {
                let effect = deadsync_song_lua::playback::actor_conformance::effect_sample(
                    state, seconds, beat,
                );
                [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = effect.rotation;
            }
            state = deadsync_song_lua::playback::actor_conformance::transform_state(
                state,
                [seconds, beat],
            );
            let actual_diffuse = state.vertex_colors.map_or(state.diffuse, |corners| {
                std::array::from_fn(|channel| state.diffuse[channel] * corners[0][channel])
            });
            if let Some(native) = native_draw_crop(sample) {
                for (channel, actual) in [
                    state.cropleft,
                    state.cropright,
                    state.croptop,
                    state.cropbottom,
                ]
                .into_iter()
                .enumerate()
                {
                    crops.check_once((native[channel] - actual).abs() <= 0.000_1, &mut reported_crops[channel], || format!("draw crop differs for {} ({definition_id}) at beat {beat:.3}, channel {channel}: ITGmania {}, DeadSync {actual}", track.actor, native[channel]));
                }
            }
            if let Some(native) = native_draw_shadow(sample) {
                let actual = [
                    state.shadow_len[0],
                    -state.shadow_len[1],
                    state.shadow_color[0],
                    state.shadow_color[1],
                    state.shadow_color[2],
                    state.shadow_color[3],
                ];
                for channel in 0..6 {
                    shadows.check_once((native[channel]-actual[channel]).abs() <= 0.000_1, &mut reported_shadows[channel], || format!("draw shadow differs for {} ({definition_id}) at beat {beat:.3}, channel {channel}: ITGmania {}, DeadSync {}", track.actor, native[channel], actual[channel]));
                }
            }
            let has_colors = sample.get(9).is_some_and(Value::is_array)
                && sample.get(10).is_some_and(Value::is_array);
            let actual_visible = state.sprite_texture
                && state.visible
                && (actual_diffuse[3] > 0.000_001 || (has_colors && state.glow[3] > 0.000_001));
            if native_visible && has_colors {
                for (field, actual) in [(9, actual_diffuse), (10, state.glow)] {
                    let native = sample[field].as_array().expect("native RGBA array");
                    for channel in 0..4 {
                        let expected = native.get(channel).and_then(projected_alpha);
                        colors.check_once(
                            expected.is_some_and(|expected| expected == actual[channel]
                                || (expected.is_nan() && actual[channel].is_nan())
                                || (expected - actual[channel]).abs() <= 0.000_1),
                            &mut reported_colors[(field - 9) * 4 + channel],
                            || format!("draw color differs for {} ({definition_id}) at beat {beat:.3}, field {field} channel {channel}: ITGmania {expected:?}, DeadSync {}", track.actor, actual[channel]),
                        );
                    }
                }
            }
            let visibility_matches = native_visible == actual_visible || {
                let probe_beat = (beat
                    + if native_visible {
                        PROJECTED_BEAT_EPSILON
                    } else {
                        -PROJECTED_BEAT_EPSILON
                    })
                .max(0.0);
                let probe_seconds = song_elapsed_seconds_at(probe_beat, context);
                state_cache
                    .entry((layer, probe_beat.to_bits(), probe_seconds.to_bits()))
                    .or_insert_with(|| {
                        compiled_overlay_states_at(
                            &compiled[layer],
                            context,
                            probe_beat,
                            probe_seconds,
                        )
                    })
                    .get(overlay_index)
                    .is_some_and(|state| {
                        native_visible
                            == (state.visible
                                && (state.diffuse[3] > 0.000_001
                                    || (has_colors && state.glow[3] > 0.000_001)))
                    })
            };
            parity.check_once(visibility_matches, &mut reported_visibility, || {
                format!(
                    "projected visibility differs for {} ({definition_id}) at beat {beat:.3}: ITGmania {native_visible}, DeadSync {actual_visible} (visible={}, alpha={:.3})",
                    track.actor, state.visible, state.diffuse[3]
                )
            });
            if native_visible && actual_visible {
                parity.check_once(
                    native_alpha == actual_diffuse[3]
                        || (native_alpha - actual_diffuse[3]).abs() <= 0.03,
                    &mut reported_alpha,
                    || {
                        format!(
                            "projected alpha differs for {} ({definition_id}) at beat {beat:.3}: ITGmania {native_alpha:.3}, DeadSync {:.3}",
                            track.actor, state.diffuse[3]
                        )
                    },
                );
            }
            if !native_visible || !actual_visible || state.stretch_rect.is_some() {
                continue;
            }
            let Some(native_vertices) = native_screen_vertices(sample) else {
                continue;
            };
            if native_vertices.len() != 4 {
                continue;
            }
            // The screen can acquire FOV after this track's first invisible
            // sample. Its camera label is fixed, while the sampled camera
            // and the compiled ScreenGameplay parent change over the song.
            let perspective = sample
                .get(7)
                .and_then(Value::as_array)
                .and_then(|camera| value_f32(camera.first()))
                .is_some_and(|fov| fov != 0.0);
            // A Quad has unit geometry, including when an older capture stores
            // its already zoomed dimensions in the texture size field.
            let size = if matches!(
                compiled[layer].overlays[overlay_index].kind,
                SongLuaOverlayKind::Quad
            ) {
                [1.0; 2]
            } else {
                track.texture_size
            };
            let actual_vertices = if track.camera_actor == "orthographic-screen" && !perspective {
                compiled_world_vertices(state, size).map(|[x, y, _, _]| [x, y])
            } else {
                let states = &state_cache[&(layer, beat.to_bits(), seconds.to_bits())];
                let Some(vertices) = compiled_perspective_vertices(
                    &compiled[layer],
                    states,
                    overlay_index,
                    state,
                    size,
                    screen_states
                        .get(&(beat.to_bits(), seconds.to_bits()))
                        .copied(),
                ) else {
                    parity.check_once(false, &mut reported_bounds, || {
                        format!("projected perspective geometry is untested for {definition_id}: missing camera or undefined perspective divide at beat {beat:.3}")
                    });
                    continue;
                };
                vertices
            };
            let expected_bounds = vertex_bounds(&native_vertices);
            let actual_bounds = vertex_bounds(&actual_vertices);
            parity.check_once(
                expected_bounds
                    .iter()
                    .zip(actual_bounds)
                    .all(|(expected, actual)| (expected - actual).abs() <= 0.75),
                &mut reported_bounds,
                || {
                    format!(
                        "projected bounds differ for {} ({definition_id}) at beat {beat:.3}: ITGmania {expected_bounds:?}, DeadSync {actual_bounds:?}",
                        track.texture
                    )
                },
            );
            let expected_center = vertex_center(&native_vertices);
            let actual_center = vertex_center(&actual_vertices);
            parity.check_once(
                expected_center
                    .iter()
                    .zip(actual_center)
                    .all(|(expected, actual)| (expected - actual).abs() <= 0.75),
                &mut reported_center,
                || {
                    format!(
                        "projected center differs for {} ({definition_id}) at beat {beat:.3}: ITGmania {expected_center:?}, DeadSync {actual_center:?}",
                        track.texture
                    )
                },
            );
        }
    }
    parity.sections.extend(colors.sections);
    parity.gaps.extend(colors.gaps);
    if crops.checks() > 0 {
        parity.sections.extend(crops.sections);
        parity.gaps.extend(crops.gaps);
    }
    if shadows.checks() > 0 {
        parity.sections.extend(shadows.sections);
        parity.gaps.extend(shadows.gaps);
    }
}

fn compare_projected_vibration_coverage(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    parity.section("projected vibration");
    let drawable_map = projected_drawable_map(trace, compiled);
    for track in &trace.projected_vertex_tracks {
        let Some(definition_id) = track.definition_id.as_deref() else {
            continue;
        };
        let Some(&(layer, overlay_index)) = drawable_map.get(&track.actor) else {
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
                    context,
                    &compiled[layer],
                    index,
                    SongLuaOverlayUpdateTarget::Vibrate,
                    beat,
                    seconds,
                )
                .and_then(|value| match value {
                    SongLuaOverlayUpdateValue::Bool(value) => Some(value),
                    _ => None,
                })
                .unwrap_or(message_state.vibrate);
                if vibrate {
                    let magnitude = compiled_update_value_at(
                        context,
                        &compiled[layer],
                        index,
                        SongLuaOverlayUpdateTarget::EffectMagnitude,
                        beat,
                        seconds,
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
            for screen_layer in compiled {
                if let Some(index) = screen_layer.screen_overlay_index {
                    let mut screen =
                        compiled_command_state_at(context, screen_layer, index, beat, seconds);
                    apply_runtime_updates(context, screen_layer, index, beat, seconds, &mut screen);
                    if screen.vibrate {
                        for axis in 0..3 {
                            actual[axis] += screen.effect_magnitude[axis];
                        }
                    }
                }
            }
            parity.check(
                native
                    .iter()
                    .zip(actual)
                    .all(|(expected, actual)| (expected - actual).abs() <= 0.03),
                || {
                    format!(
                        "projected vibration differs for {definition_id} at beat {beat:.3}: ITGmania {native:?}, DeadSync {actual:?}"
                    )
                },
            );
        }
    }
}

/// One check per capture category and layer: DeadSync must have compiled the
/// complete script without skipping anything it could not model.
fn compare_compile_info(compiled: &[CompiledSongLua], parity: &mut Parity) {
    parity.section("compile info");
    for (layer, compiled) in compiled.iter().enumerate() {
        let info = &compiled.info;
        for (kind, details) in [
            (
                "unsupported function ease",
                &info.unsupported_function_ease_captures,
            ),
            (
                "unsupported function action",
                &info.unsupported_function_action_captures,
            ),
            ("unsupported perframe", &info.unsupported_perframe_captures),
            (
                "skipped message command",
                &info.skipped_message_command_captures,
            ),
        ] {
            if details.is_empty() {
                parity.check(true, String::new);
            }
            for detail in details {
                parity.check(false, || format!("layer {layer} {kind}: {detail}"));
            }
        }
    }
}

/// Runs every semantic comparator in report order.
fn compare_semantics(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    primary_index: usize,
    context: &SongLuaCompileContext,
) -> Parity {
    let mut parity = Parity::default();
    compare_compile_info(compiled, &mut parity);
    compare_layers(trace, compiled, &mut parity);
    compare_final_render_states(trace, compiled, context, &mut parity);
    compare_player_proxy_sources(trace, compiled, &mut parity);
    compare_judgment_textures(trace, &compiled[primary_index], context, &mut parity);
    compare_sprite_textures(trace, compiled, context, &mut parity);
    compare_update_render_persistence(context, trace, compiled, &mut parity);
    compare_update_render_values(trace, compiled, context, &mut parity);
    compare_player_operation_ranges(trace, compiled, &mut parity);
    compare_column_splines(trace, compiled, context, &mut parity);
    multitap::compare_multitap(trace, compiled, context, &mut parity);
    compare_projected_geometry(trace, compiled, context, &mut parity);
    compare_manual_meshes(trace, compiled, context, &mut parity);
    compare_projected_vibration_coverage(trace, compiled, context, &mut parity);
    compare_timeline(trace, &compiled[primary_index], &mut parity);
    compare_commands(trace, compiled, primary_index, &mut parity);
    parity
}

/// Explicit native mesh Draw calls must reach the production textured draw
/// pipeline. This exposes lost AFT bindings even when actor-state checks pass.
fn compare_manual_meshes(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    use deadlib_present::render::DrawOp;
    use deadlib_present::render::render_target_base_handle;
    use deadsync_song_lua::playback::actor_conformance::{
        WholeSongComposer, matrix_rows, project_world,
    };
    if trace.manual_draw_frames.is_empty() {
        return;
    }
    compare_manual_plans(trace, compiled, parity);
    parity.section("manual mesh bindings");
    let map = projected_drawable_map(trace, compiled);
    let mut composers = compiled
        .iter()
        .map(|layer| {
            let mut composer = WholeSongComposer::new(&layer.overlays);
            composer.set_draw_frames(&layer.overlays, &layer.draw_frames);
            composer
        })
        .collect::<Vec<_>>();
    let mut reported = HashSet::new();
    let mut poses = Parity::default();
    poses.section("manual mesh poses");
    let mut colors = Parity::default();
    colors.section("manual mesh colors");
    let mut pose_reported = HashSet::new();
    let mut color_reported = HashSet::new();
    let screen = [context.screen_width, context.screen_height];
    for (beat, second, calls) in &trace.manual_draw_frames {
        let mut states = HashMap::new();
        let mut rendered = HashMap::new();
        for call in calls
            .iter()
            .filter(|call| call["class"] == "ActorMultiVertex")
        {
            let Some(texture) = call["texture"].as_str() else {
                continue;
            };
            let Some(target) = texture.strip_prefix("aft:") else {
                continue;
            };
            let Some(actor) = call["actor"].as_str() else {
                continue;
            };
            let mut failed = reported.contains(actor);
            let Some(&(layer, index)) = map.get(actor) else {
                parity.check_once(false, &mut failed, || {
                    format!("manual mesh {actor} is missing from compiled overlays")
                });
                reported.insert(actor.to_owned());
                continue;
            };
            let expected_handle = map.get(target).and_then(|&(target_layer, target_index)| {
                (target_layer == layer)
                    .then(|| composers[layer].capture_handle(target_index))
                    .flatten()
            });
            let states = states.entry(layer).or_insert_with(|| {
                compiled_overlay_states_at(&compiled[layer], context, *beat as f32, *second as f32)
            });
            let frames = rendered.entry(layer).or_insert_with(|| {
                std::collections::VecDeque::from(composers[layer].render_manual_frame(
                    &compiled[layer].overlays,
                    states,
                    screen,
                    *second as f32,
                    *beat as f32,
                ))
            });
            let frame = frames
                .iter()
                .position(|(overlay, _)| *overlay == index)
                .and_then(|position| frames.remove(position))
                .map(|(_, frame)| frame);
            let Some(frame) = frame else {
                parity.check_once(false, &mut failed, || {
                    format!("manual mesh {actor} has no rendered pass at beat {beat:.3}")
                });
                poses.check(false, || {
                    format!("manual mesh {actor} has no rendered pose at beat {beat:.3}")
                });
                colors.check(false, || {
                    format!("manual mesh {actor} has no rendered color at beat {beat:.3}")
                });
                continue;
            };
            let bound = expected_handle.is_some_and(|expected| frame.ops.iter().any(|op| {
                matches!(op, DrawOp::TexturedMesh(run) if render_target_base_handle(run.texture_handle) == expected)
            }));
            parity.check_once(bound, &mut failed, || {
                format!("manual mesh {actor} loses capture identity {target} at beat {beat:.3}")
            });
            if failed {
                reported.insert(actor.to_owned());
            }
            let mut actual = Vec::new();
            for op in &frame.ops {
                let DrawOp::TexturedMesh(run) = op else {
                    continue;
                };
                for instance in &frame.tmesh_instances[run.instance_start as usize..]
                    [..run.instance_count as usize]
                {
                    let matrix =
                        matrix_rows(frame.cameras[usize::from(run.camera)] * instance.transform());
                    for vertex in frame.tmesh_geometries[run.geometry as usize]
                        .vertices
                        .iter()
                    {
                        let clip = project_world(
                            matrix,
                            [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0],
                        );
                        let position = [
                            (clip[0] / clip[3] + 1.0) * screen[0] * 0.5,
                            (1.0 - clip[1] / clip[3]) * screen[1] * 0.5,
                        ];
                        let color = std::array::from_fn::<_, 4, _>(|axis| {
                            vertex.color[axis] * instance.tint[axis]
                        });
                        actual.push((position, vertex.uv, color));
                    }
                }
            }
            let mut pose_ok = true;
            let mut color_ok = true;
            let mut native_vertices = 0;
            for primitive in call["primitives"].as_array().into_iter().flatten() {
                if primitive["texture_mode"] != "modulate" {
                    continue;
                }
                for vertex in primitive["vertices"].as_array().into_iter().flatten() {
                    native_vertices += 1;
                    let uv = [
                        vertex["uv"][0].as_f64().unwrap_or(f64::NAN) as f32,
                        vertex["uv"][1].as_f64().unwrap_or(f64::NAN) as f32,
                    ];
                    let expected = [
                        vertex["screen"][0].as_f64().unwrap_or(f64::NAN) as f32,
                        vertex["screen"][1].as_f64().unwrap_or(f64::NAN) as f32,
                    ];
                    let matches = actual
                        .iter()
                        .filter(|(_, actual_uv, _)| {
                            actual_uv
                                .iter()
                                .zip(uv)
                                .all(|(a, b)| (a - b).abs() <= 0.00001)
                        })
                        .collect::<Vec<_>>();
                    pose_ok &= matches.iter().any(|(position, _, _)| {
                        position
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| (a - b).abs() <= 0.75)
                    });
                    color_ok &= matches.iter().any(|(_, _, color)| {
                        color.iter().enumerate().all(|(axis, actual)| {
                            let expected =
                                vertex["color"][axis].as_f64().unwrap_or(f64::NAN) as f32;
                            (actual.clamp(0.0, 1.0) * 255.0 - expected).abs() <= 1.01
                        })
                    });
                }
            }
            let mut failed = pose_reported.contains(actor);
            poses.check_once(native_vertices > 0 && pose_ok, &mut failed, || {
                format!("manual mesh {actor} loses a draw pose or UV at beat {beat:.3}")
            });
            if failed {
                pose_reported.insert(actor.to_owned());
            }
            let mut failed = color_reported.contains(actor);
            colors.check_once(native_vertices > 0 && color_ok, &mut failed, || {
                format!("manual mesh {actor} loses a draw color at beat {beat:.3}")
            });
            if failed {
                color_reported.insert(actor.to_owned());
            }
        }
    }
    parity.sections.extend(poses.sections);
    parity.sections.extend(colors.sections);
    parity.gaps.extend(poses.gaps);
    parity.gaps.extend(colors.gaps);
}

fn compare_manual_plans(trace: &NativeTrace, compiled: &[CompiledSongLua], parity: &mut Parity) {
    use deadsync_song_lua::{SongLuaDrawOp as Op, SongLuaDrawSource as Source};
    parity.section("custom draw plan");
    let map = projected_drawable_map(trace, compiled);
    let mut reported = false;
    for (beat, second, calls) in &trace.manual_draw_frames {
        let mut actual = Vec::new();
        for (layer, compiled) in compiled.iter().enumerate() {
            let end = compiled
                .draw_frames
                .partition_point(|frame| frame.second <= *second as f32 + 0.0001);
            if let Some(frame) = end.checked_sub(1).map(|index| &compiled.draw_frames[index]) {
                actual.extend(frame.ops.iter().map(|op| (layer, op)));
            }
        }
        let source_matches = |call: &Value, layer, source| match source {
            Source::Overlay(index) => {
                call["actor"].as_str().and_then(|id| map.get(id)) == Some(&(layer, index))
            }
            Source::Player(index) => call["path"] == format!("ScreenGameplay/PlayerP{}", index + 1),
            Source::Judgment(index) => {
                call["path"] == format!("ScreenGameplay/PlayerP{}/Judgment", index + 1)
            }
            Source::Combo(index) => {
                call["path"] == format!("ScreenGameplay/PlayerP{}/Combo", index + 1)
            }
            Source::SongForeground => call["path"] == "ScreenGameplay/SongForeground",
            Source::ScreenLayer(index) => {
                call["path"]
                    == [
                        "ScreenGameplay/Underlay",
                        "ScreenGameplay/Overlay",
                        "ScreenGameplay/SongBackground",
                        "ScreenGameplay/In",
                    ][index]
            }
        };
        let matches = actual.len() == calls.len()
            && actual
                .iter()
                .zip(calls)
                .all(|((layer, op), call)| match op {
                    Op::Begin { capture, preserve } => {
                        call["operation"] == "begin"
                            && call["target"].as_str().and_then(|id| map.get(id))
                                == Some(&(*layer, *capture))
                            && call["preserve"]
                                .as_bool()
                                .is_none_or(|native| native == *preserve)
                    }
                    Op::Finish { capture } => {
                        call["operation"] == "finish"
                            && call["target"].as_str().and_then(|id| map.get(id))
                                == Some(&(*layer, *capture))
                    }
                    Op::Draw { source, .. } => {
                        call["operation"] == "draw" && source_matches(call, *layer, *source)
                    }
                });
        parity.check_once(matches, &mut reported, || {
            let native = calls.iter().map(|call| format!("{}:{}",
                call["operation"].as_str().unwrap_or("?"),
                call["path"].as_str().or_else(|| call["target"].as_str()).unwrap_or("?")))
                .collect::<Vec<_>>().join(", ");
            let retained = actual.iter().map(|(layer, op)| match op {
                Op::Begin { capture, .. } => format!("begin:{layer}/{capture}"),
                Op::Finish { capture } => format!("finish:{layer}/{capture}"),
                Op::Draw { source, .. } => format!("draw:{layer}/{source:?}"),
            }).collect::<Vec<_>>().join(", ");
            format!("custom draw order/sources differ at beat {beat:.3}: {} native operations vs {} retained\nnative: {native}\nretained: {retained}", calls.len(), actual.len())
        });
    }
}

fn compare_sprite_textures(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    let actors = projected_drawable_map(trace, compiled);
    let mut loads = HashMap::<&str, Vec<(u64, f32, &str)>>::new();
    for track in &trace.tween_tracks {
        for segment in &track.segments {
            for operation in &segment.operations {
                if operation.operation == "Sprite.Load"
                    && let Some(path) = operation.args.first().and_then(Value::as_str)
                {
                    let second = segment
                        .seconds
                        .unwrap_or_else(|| song_elapsed_seconds_at(segment.beat, context));
                    loads
                        .entry(&track.actor)
                        .or_default()
                        .push((operation.seq, second, path));
                }
            }
        }
    }
    for track in &trace.operation_tracks {
        if track.operation == "Sprite.Load" && track.actor.starts_with("def-") {
            for (seq, _, second, args) in &track.samples {
                if let Some(path) = args.first().and_then(Value::as_str) {
                    loads
                        .entry(&track.actor)
                        .or_default()
                        .push((*seq, *second, path));
                }
            }
        }
    }
    // SetTexture receives a RageTexture handle, so setter arguments alone do
    // not identify its image. The native draw capture retains immutable source
    // paths on every binding/size change, including copies from hidden caches.
    for track in &trace.projected_vertex_tracks {
        for (_, second, path, _, _) in &track.texture_samples {
            if !path.starts_with("aft:") {
                loads
                    .entry(&track.actor)
                    .or_default()
                    .push((u64::MAX, *second, path));
            }
        }
    }
    if loads.is_empty() {
        return;
    }
    parity.section("sprite textures");
    for (actor, mut expected) in loads {
        expected.sort_by_key(|sample| sample.0);
        let Some(&(layer, index)) = actors.get(actor) else {
            parity.check(false, || {
                format!("Sprite.Load has no matching DeadSync actor: {actor}")
            });
            continue;
        };
        let SongLuaOverlayKind::Sprite {
            texture_key,
            texture_path,
            textures,
            ..
        } = &compiled[layer].overlays[index].kind
        else {
            parity.check(false, || {
                format!("Sprite.Load actor {actor} does not render a sprite")
            });
            continue;
        };
        for (_, second, raw) in expected {
            let path = raw.strip_prefix("song:/").map_or_else(
                || {
                    if Path::new(raw).is_absolute() {
                        PathBuf::from(raw)
                    } else {
                        context.song_dir.join(raw)
                    }
                },
                |relative| context.song_dir.join(relative),
            );
            let expected_path = fs::canonicalize(&path);
            let active = deadsync_song_lua::sprite_texture_at(textures, second + 1e-5);
            let actual = active.map_or(texture_path, |texture| &texture.path);
            let key = active.map_or(texture_key.as_ref(), |texture| texture.key.as_ref());
            parity.check(expected_path.as_ref().is_ok_and(|path|
                fs::canonicalize(actual).is_ok_and(|actual| actual == *path)
                    && fs::canonicalize(key).is_ok_and(|key| key == *path)),
                || format!("Sprite.Load binding differs for {actor} at {second:.6}s: ITGmania {path:?}, DeadSync {actual:?}, render key {key:?}"));
        }
    }
}

fn compare_judgment_textures(
    trace: &NativeTrace,
    compiled: &CompiledSongLua,
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    parity.section("judgment textures");
    for player in 0..2 {
        let path = format!(
            "ScreenGameplay/PlayerP{}/Judgment/JudgmentWithOffsets",
            player + 1
        );
        let Some(actor) = trace
            .external_actors
            .iter()
            .find(|actor| actor.path == path)
        else {
            continue;
        };
        let child = &compiled.player_actors[player].judgment;
        let mut expected = trace
            .operation_tracks
            .iter()
            .filter(|track| track.actor == actor.id && track.operation == "Sprite.Load")
            .flat_map(|track| &track.samples)
            .collect::<Vec<_>>();
        expected.sort_by_key(|sample| sample.0);
        if expected.is_empty() && child.textures.is_empty() {
            continue;
        }
        parity.check(child.textures.len() == expected.len(), || {
            format!(
                "PlayerP{} judgment sheet count: ITGmania {}, DeadSync {}",
                player + 1,
                expected.len(),
                child.textures.len()
            )
        });
        for (_, _, second, args) in expected {
            let raw = args
                .first()
                .and_then(Value::as_str)
                .expect("Sprite.Load path");
            let expected_path = if let Some(relative) = raw.strip_prefix("song:/") {
                context.song_dir.join(relative)
            } else if Path::new(raw).is_absolute() {
                PathBuf::from(raw)
            } else {
                context.song_dir.join(raw)
            };
            let expected_path = fs::canonicalize(&expected_path)
                .expect("native judgment sheet exists in the local fixture");
            let actual = deadsync_song_lua::judgment_texture_at(child, second + 1e-5);
            parity.check(actual.is_some_and(|texture| {
                fs::canonicalize(&texture.path).is_ok_and(|path| path == expected_path)
                    && (texture.second - second).abs() < 1e-4
            }), || format!("PlayerP{} judgment sheet at {second:.6}s: ITGmania {expected_path:?}, DeadSync {actual:?}", player + 1));
        }
    }
}

#[test]
#[ignore = "reports the current known song-Lua parity gaps"]
fn native_song_lua_semantics_match_deadsync() {
    crate::paths::init();
    let trace = read_trace();
    assert_eq!(trace.oracle, "itgmania_song_lua_headless_semantic_trace");
    let (compiled, primary_index, context) = compile_trace_song(&trace);
    eprintln!(
        "compiled {} layer(s): {} overlays, {} overlay eases, {} overlay update tracks, {} beat mods, {} player eases, {} column offsets, {} messages; unsupported: {} function eases, {} function actions, {} perframes, {} skipped message commands",
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
            .map(|layer| layer.eases.len())
            .sum::<usize>(),
        compiled
            .iter()
            .map(|layer| layer.column_offsets.len())
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
    let mut parity = compare_semantics(&trace, &compiled, primary_index, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("song Lua semantic");
}

#[test]
fn last_parent_write_waits_for_child_update() {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/last-child-color.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/last-child-color.sm"),
    );
    compare_semantics(&trace, &compiled, primary, &context).assert_complete("last child color");
    let beat = trace.trace_until_beat;
    let second = trace.end_position.seconds;
    let states = compiled_overlay_states_at(&compiled[primary], &context, beat, second);
    let index = compiled[primary]
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Color"))
        .unwrap();
    assert!(
        (states[index].diffuse[3] - 0.99).abs() <= 0.0001,
        "last draw keeps the prior callback's color"
    );
    let mut composer = WholeSongComposer::new(&compiled[primary].overlays);
    let frame = composer.render_overlay(
        &compiled[primary].overlays,
        &states,
        index,
        [854.0, 480.0],
        second,
        beat,
    );
    assert!(
        !frame.ops.is_empty(),
        "pending transparent destination must not hide the last draw"
    );
    let mut wrong = compiled.clone();
    for update in &mut wrong[primary].overlay_updates {
        if update.overlay_index == index && update.target == SongLuaOverlayUpdateTarget::Diffuse {
            update.samples.last_mut().unwrap().value = SongLuaOverlayUpdateValue::Vec4([0.0; 4]);
        }
    }
    let mut rejected = Parity::default();
    compare_projected_geometry(&trace, &wrong, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "premature final destination must fail"
    );
}

#[test]
fn shadow_setters_bypass_child_tweens() {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/immediate-shadow.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/immediate-shadow.sm"),
    );
    compare_semantics(&trace, &compiled, primary, &context).assert_complete("immediate shadows");
    let index = compiled[primary]
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Shadow"))
        .expect("shadow quad");
    let mut composer = WholeSongComposer::new(&compiled[primary].overlays);
    let sample = trace.projected_vertex_tracks[0]
        .samples
        .iter()
        .find(|sample| {
            sample[1]
                .as_f64()
                .is_some_and(|time| time > 0.5 && time < 0.8)
                && sample[12][0].as_f64().is_some_and(|x| x > 1.0)
        })
        .expect("visible shadow sample")
        .as_array()
        .expect("sample");
    let beat = value_f32(sample.first()).expect("beat");
    let second = value_f32(sample.get(1)).expect("seconds");
    let states = compiled_overlay_states_at(&compiled[primary], &context, beat, second);
    let frame = composer.render_overlay(
        &compiled[primary].overlays,
        &states,
        index,
        [854.0, 480.0],
        second,
        beat,
    );
    let actual = rendered_quad_corners(&frame, [854.0, 480.0]);
    let mut expected = native_screen_vertices(sample).expect("native quad corners");
    let shadow = native_draw_shadow(sample).expect("native current shadow");
    assert!(shadow[0] > 1.0 && shadow[1] > 1.0);
    expected.extend(
        expected
            .clone()
            .into_iter()
            .map(|[x, y]| [x + shadow[0], y + shadow[1]]),
    );
    for vertex in &expected {
        assert!(actual.iter().any(|got| (got[0]-vertex[0]).abs() < 0.002 && (got[1]-vertex[1]).abs() < 0.002), "missing native shadow corner {vertex:?}, rendered {actual:?}");
    }
    let end = compiled_overlay_states_at(
        &compiled[primary],
        &context,
        trace.trace_until_beat,
        trace.end_position.seconds,
    );
    assert_eq!(
        end[index].shadow_len,
        [0.0, -0.0],
        "the final callback changes its shadow immediately"
    );
    assert!(
        (end[index].diffuse[3] - 0.99).abs() < 0.0001,
        "the final callback's diffuse remains queued"
    );
    let mut wrong = compiled.clone();
    wrong[primary].overlay_updates.retain(|update| {
        !matches!(
            update.target,
            SongLuaOverlayUpdateTarget::ShadowLen | SongLuaOverlayUpdateTarget::ShadowColor
        )
    });
    let mut rejected = Parity::default();
    compare_projected_geometry(&trace, &wrong, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "missing current shadow writes must fail"
    );
}

#[test]
fn image_texture_aliases_match_native_draws() {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/texture-alias.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/texture-alias.sm"),
    );
    compare_semantics(&trace, &compiled, primary, &context).assert_complete("texture aliases");
    let layer = &compiled[primary];
    let index = |name| {
        layer
            .overlays
            .iter()
            .position(|actor| actor.name.as_deref() == Some(name))
            .unwrap()
    };
    let first = index("First");
    let second = index("Second");
    let mut composer = WholeSongComposer::new(&layer.overlays);
    let screen = [context.screen_width, context.screen_height];
    let mut handles = Vec::new();
    // Exercise the actual gameplay builder, including a backward seek after the
    // first alias switches image; the second alias must retain its old image.
    for time in [0.0, 0.2, 0.0] {
        let states = compiled_overlay_states_at(layer, &context, time, time);
        let mut pair = Vec::new();
        for (index, center, height) in [
            (first, 100.0, if time == 0.2 { 64.0 } else { 32.0 }),
            (second, 200.0, 32.0),
        ] {
            let frame =
                composer.render_overlay(&layer.overlays, &states, index, screen, time, time);
            let corners = rendered_quad_corners(&frame, screen);
            assert!(!corners.is_empty(), "aliased sprite must draw");
            let expected = [
                center - 32.0,
                100.0 - height / 2.0,
                center + 32.0,
                100.0 + height / 2.0,
            ];
            let bounds = vertex_bounds(&corners);
            assert!(
                bounds
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| (actual - expected).abs() <= 0.002),
                "{time}: {bounds:?} vs {expected:?}"
            );
            let handle = frame
                .ops
                .iter()
                .find_map(|op| match op {
                    deadlib_present::render::DrawOp::Sprite(run) => Some(run.texture_handle),
                    deadlib_present::render::DrawOp::TexturedMesh(run) => Some(run.texture_handle),
                    _ => None,
                })
                .expect("textured draw");
            pair.push(handle);
        }
        handles.push(pair);
    }
    assert_eq!(handles[0][0], handles[0][1]);
    assert_ne!(handles[1][0], handles[1][1]);
    assert_eq!(
        handles[0], handles[2],
        "backward seek restores both bindings"
    );
    let mut missing = compiled.clone();
    if let SongLuaOverlayKind::Sprite { textures, .. } = &mut missing[primary].overlays[first].kind
    {
        *textures = Default::default();
    }
    let mut rejected = Parity::default();
    compare_sprite_textures(&trace, &missing, &context, &mut rejected);
    assert!(!rejected.gaps.is_empty(), "missing alias swaps must fail");
}

#[test]
fn hidden_cache_keeps_membership_and_visible_order() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/hidden-cache-order.json.zst"),
    );
    let (mut compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/hidden-cache-order.sm"),
    );
    let index = |name| {
        compiled[primary]
            .overlays
            .iter()
            .position(|actor| actor.name.as_deref() == Some(name))
            .unwrap()
    };
    let (hidden_a, hidden_b, visible_a, visible_b) = (
        index("HiddenA"),
        index("HiddenB"),
        index("VisibleA"),
        index("VisibleB"),
    );
    compiled[primary].overlays.swap(hidden_a, hidden_b);
    compare_semantics(&trace, &compiled, primary, &context).assert_complete("hidden cache order");
    for control in 0..4 {
        let mut wrong = compiled.clone();
        match control {
            0 => wrong[primary].overlays.swap(visible_a, visible_b),
            1 => {
                wrong[primary].overlays.remove(hidden_a);
            }
            2 => wrong[primary].overlays[hidden_a].name = Some("missing resource".into()),
            _ => wrong[primary].overlays[hidden_a].initial_state.visible = true,
        }
        let mut rejected = Parity::default();
        compare_layers(&trace, &wrong, &mut rejected);
        if control == 3 {
            compare_final_render_states(&trace, &wrong, &context, &mut rejected);
        }
        assert!(
            !rejected.gaps.is_empty(),
            "visible/resource countercheck {control} must fail"
        );
    }
}

#[test]
fn startup_delay_matches_native_dispatch() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/startup-delay.json"));
    let simfile = root.join("crates/deadsync-song-lua/tests/fixtures/startup-delay.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    let expected = trace
        .timeline_tracks
        .iter()
        .flat_map(|track| &track.samples)
        .find(|sample| sample.3.first().is_some_and(|name| name == "ShowAnswer"))
        .expect("native timeout")
        .1
        .expect("native timeout beat");
    let actual = compiled[primary]
        .messages
        .iter()
        .find(|message| message.message == "ShowAnswer")
        .expect("compiled timeout")
        .beat;
    assert!(
        (expected - actual).abs() <= 0.0001,
        "{expected} vs {actual}"
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    parity.assert_complete("startup delay");
}

#[test]
fn foreground_delay_matches_native_visibility() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/foreground-delay.json"));
    let simfile = root.join("crates/deadsync-song-lua/tests/fixtures/foreground-delay.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    assert!(compiled[primary].song_foreground.initial_state.visible);
    assert!(
        !compiled[primary]
            .song_foreground
            .message_commands
            .is_empty()
    );
    compare_semantics(&trace, &compiled, primary, &context).assert_complete("foreground delay");
}

#[test]
fn consumed_message_checks_runtime_timeline() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/consumed-message.json"));
    let simfile = root.join("crates/deadsync-song-lua/tests/fixtures/consumed-message.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    assert!(
        compiled[primary]
            .overlays
            .iter()
            .flat_map(|actor| &actor.message_commands)
            .all(|command| command.message != "Consume")
    );
    compare_semantics(&trace, &compiled, primary, &context).assert_complete("consumed message");
    let mut missing = compiled[primary].clone();
    missing.messages.retain(|event| event.message != "Consume");
    let mut parity = Parity::default();
    compare_timeline(&trace, &missing, &mut parity);
    assert_eq!(
        parity.gaps.len(),
        1,
        "lost runtime-only broadcasts must fail: {}",
        parity.summary("consumed message")
    );
}

#[test]
fn queued_broadcasts_match_native_frames() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, checks) in [("nested-start", 39), ("nested-start-stop", 34)] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json"
        )));
        let simfile = root.join(format!("crates/deadsync-song-lua/tests/fixtures/{name}.sm"));
        let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
        let native = trace
            .timeline_tracks
            .iter()
            .flat_map(|track| &track.samples)
            .find(|sample| {
                sample
                    .3
                    .first()
                    .is_some_and(|name| name == "BodyRotateBuildings")
            })
            .expect("native queued broadcast")
            .1
            .expect("native broadcast beat");
        let actual = compiled[primary]
            .messages
            .iter()
            .find(|message| message.message == "BodyRotateBuildings")
            .expect("compiled queued broadcast")
            .beat;
        assert!(
            (native - actual).abs() <= 0.0001,
            "{name}: {native} vs {actual}"
        );
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        assert_eq!(parity.checks(), checks);
        parity.assert_complete(&trace.title);
    }
}

#[test]
fn queued_lua_state_matches_native_dispatch() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["queued-state", "queued-local-state"] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json"
        )));
        let simfile = root.join(format!("crates/deadsync-song-lua/tests/fixtures/{name}.sm"));
        let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        parity.assert_complete(name);
    }
}

#[test]
fn recurring_road_loop_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, checks) in [
        ("road-loop", 1900),
        ("road-loop-order", 2847),
        ("road-loop-parent", 3794),
    ] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json.zst"
        )));
        let simfile = root.join(format!("crates/deadsync-song-lua/tests/fixtures/{name}.sm"));
        let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        assert_eq!(parity.checks(), checks);
        parity.assert_complete(name);
    }
}

#[test]
fn recurring_visibility_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/recurring-visible.json.zst"),
    );
    let simfile = root.join("crates/deadsync-song-lua/tests/fixtures/recurring-visible.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Recurring visibility"));
    assert_eq!(parity.checks(), 114);
    parity.assert_complete("Recurring visibility");
}

#[test]
fn recurring_stop_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/recurring-stop.json.zst"),
    );
    let simfile = root.join("crates/deadsync-song-lua/tests/fixtures/recurring-stop.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    let driver = compiled[primary]
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Driver"))
        .expect("recurring driver");
    let x = compiled[primary]
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == driver && track.target == SongLuaOverlayUpdateTarget::X
        })
        .expect("driver call count");
    assert_eq!(
        x.samples.last().expect("last driver position").value,
        SongLuaOverlayUpdateValue::F32(151.0),
        "native stops after 151 calls, without an explicit stoptweening"
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Recurring stop"));
    assert_eq!(parity.checks(), 1632);
    parity.assert_complete("Recurring stop");
}

#[test]
fn simply_love_receptor_metrics_match_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/receptor-metrics.json.zst"),
    );
    let simfile = root.join("crates/deadsync-song-lua/tests/fixtures/receptor-metrics.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Receptor metrics"));
    assert_eq!(parity.checks(), 36);
    parity.assert_complete("Receptor metrics");
}

#[test]
fn paused_updates_match_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/paused-updates.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/paused-updates.ssc"),
    );
    assert!(
        compiled
            .iter()
            .all(|layer| layer.overlay_update_unit == SongLuaTimeUnit::Second)
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Paused updates"));
    assert!(parity.checks() > 200);
    parity.assert_complete("Paused updates");
}

#[test]
fn queued_bounce_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/queued-bounce.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/queued-bounce.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Queued bounce"));
    assert_eq!(parity.checks(), 543);
    parity.assert_complete("Queued bounce");
}

#[test]
fn player_tail_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/player-tail.json.zst"));
    let (compiled, primary, context) =
        compile_trace_song_at(&trace, &root.join("tests/fixtures/song-lua/player-tail.sm"));
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    assert_eq!(parity.checks(), 7);
    parity.assert_complete("player position crossing baseline");
    let range = compiled_player_range(&compiled, 2, &SongLuaEaseTarget::PlayerX, 0.0);
    assert_eq!(range.1, 773.9375);
}

#[test]
fn near_camera_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/near-camera-clock.json.zst"),
    );
    let (compiled, primary, context) =
        compile_trace_song_at(&trace, &root.join("tests/fixtures/song-lua/near-camera.sm"));
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("near camera"));
    assert_eq!(parity.checks(), 2374);
    parity.assert_complete("near camera");
    let native: Value =
        serde_json::from_reader(
            zstd::stream::read::Decoder::new(
                fs::File::open(root.join(
                    "tests/fixtures/itgmania-song-lua-micro/near-camera-clock-native.json.zst",
                ))
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(native["oracle"], "itgmania_native_actor_conformance");
    let index = compiled[primary]
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Tween"))
        .unwrap();
    let mut checks = 0;
    for sample in native["samples"].as_array().unwrap() {
        let second = sample["time"].as_f64().unwrap() as f32;
        let local = compiled_local_states_at(&compiled[primary], &context, second, second);
        let actor = sample["actors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|actor| actor["name"] == "Tween")
            .unwrap();
        // Exact depth catches fused arithmetic that ordinary screen bounds miss.
        assert_eq!(
            local[index].z.to_bits(),
            (actor["current"]["position"][2].as_f64().unwrap() as f32).to_bits()
        );
        let states = compiled_overlay_states_at(&compiled[primary], &context, second, second);
        let vertices = compiled_perspective_vertices(
            &compiled[primary],
            &states,
            index,
            states[index],
            [64.0, 64.0],
            None,
        )
        .expect("finite perspective vertices");
        for (corner, actual) in vertices.iter().enumerate() {
            let expected = &actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]]["screen"];
            for axis in 0..2 {
                let expected = expected[axis].as_f64().unwrap() as f32;
                assert!(
                    (actual[axis] - expected).abs() <= 0.75,
                    "at {second}, corner {corner}/{axis}: {} vs {expected}",
                    actual[axis]
                );
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 816);
}

#[test]
fn vibrate_restart_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/vibration-restart.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/vibration-restart.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("vibration restart"));
    assert_eq!(parity.checks(), 86);
    parity.assert_complete("vibration restart");
    let native: Value = serde_json::from_slice(
        &fs::read(
            root.join("tests/fixtures/itgmania-song-lua-micro/vibration-restart-native.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let index = compiled[primary]
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Vibration"))
        .unwrap();
    for sample in native["samples"].as_array().unwrap() {
        let seconds = sample["time"].as_f64().unwrap() as f32;
        let mut state =
            compiled_command_state_at(&context, &compiled[primary], index, seconds, seconds);
        apply_runtime_updates(
            &context,
            &compiled[primary],
            index,
            seconds,
            seconds,
            &mut state,
        );
        let effect = &sample["actors"][0]["effect"];
        assert_eq!(state.vibrate, effect["type"] == "vibrate");
        for axis in 0..3 {
            assert_eq!(
                state.effect_magnitude[axis],
                effect["magnitude"][axis].as_f64().unwrap() as f32
            );
        }
    }
}

#[test]
fn wrapper_transform_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/wrapper-transform.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/wrapper-transform.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Wrapper transform"));
    assert_eq!(parity.checks(), 127);
    parity.assert_complete(&trace.title);
    assert_eq!(
        compiled[0]
            .overlays
            .iter()
            .filter(|actor| matches!(actor.kind, SongLuaOverlayKind::WrapperState))
            .count(),
        4
    );
}

#[test]
fn wrapper_transform_matches_native_drawing() {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/wrapper-transform.json.zst"),
    );
    let (compiled, _, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/wrapper-transform.sm"),
    );
    let native: Value = serde_json::from_slice(
        &fs::read(
            root.join("tests/fixtures/itgmania-song-lua-micro/wrapper-transform-native.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(native["oracle"], "itgmania_native_actor_conformance");
    let mut composer = WholeSongComposer::new(&compiled[0].overlays);
    let screen = [context.screen_width, context.screen_height];
    let samples = native["samples"].as_array().unwrap();
    let mut checks = 0;
    // Reuse the warmed builder and include a backward seek through the tween.
    for sample in samples.iter().chain(samples.iter().take(3)) {
        let second = sample["time"].as_f64().unwrap() as f32;
        let states = compiled_overlay_states_at(&compiled[0], &context, second, second);
        for actor in sample["actors"].as_array().unwrap().iter().skip(1) {
            let name = actor["name"].as_str().unwrap();
            let index = compiled[0]
                .overlays
                .iter()
                .position(|a| a.name.as_deref() == Some(name))
                .unwrap();
            let actual = compiled_world_vertices(states[index], [64.0, 32.0]);
            for corner in 0..4 {
                for axis in 0..3 {
                    let expected = actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]]["world"]
                        [axis]
                        .as_f64()
                        .unwrap() as f32;
                    assert!(
                        (actual[corner][axis] - expected).abs() <= 0.002,
                        "{name} at {second}, corner {corner}/{axis}: {} vs {expected}",
                        actual[corner][axis]
                    );
                    checks += 1;
                }
            }
            let frame = composer.render_overlay(
                &compiled[0].overlays,
                &states,
                index,
                screen,
                second,
                second,
            );
            assert!(
                !frame.tmesh_instances.is_empty(),
                "{name} at {second} must render"
            );
            let expected_alpha = actor["draws"][0]["vertices"][0]["color"][3]
                .as_f64()
                .unwrap() as f32
                / 255.0;
            assert!(
                frame
                    .tmesh_instances
                    .iter()
                    .all(|instance| (instance.tint[3] - expected_alpha).abs() <= 1.0 / 255.0)
            );
        }
    }
    assert_eq!(checks, 324);
}

#[test]
fn collapsed_transform_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/collapsed-transform.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/collapsed-transform.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    assert_eq!(parity.checks(), 27);
    parity.assert_complete("Collapsed transform");
}

#[test]
fn queued_message_states_match_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, capture, checks) in [
        ("message-queue-reset", "message-queue-reset-clock", 951),
        ("queued-command-state", "queued-command-state", 33),
    ] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{capture}.json.zst"
        )));
        let (compiled, primary, context) = compile_trace_song_at(
            &trace,
            &root.join(format!("tests/fixtures/song-lua/{name}.sm")),
        );
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        assert_eq!(parity.checks(), checks);
        parity.assert_complete(name);
    }
}

#[test]
fn queued_reset_matches_native_clock() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/message-queue-reset-clock.json.zst"),
    );
    let (compiled, _, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/message-queue-reset.sm"),
    );
    let native: serde_json::Value =
        serde_json::from_reader(
            zstd::stream::read::Decoder::new(
                fs::File::open(root.join(
                    "tests/fixtures/itgmania-song-lua-micro/queued-reset-clock-native.json.zst",
                ))
                .expect("native queued reset capture"),
            )
            .expect("compressed native reset capture"),
        )
        .expect("native queued reset capture");
    let mut checks = 0;
    for sample in native["samples"].as_array().expect("native samples") {
        let seconds = value_f32(sample.get("time")).expect("native time");
        if seconds < 3.0 {
            continue;
        }
        let states = compiled_overlay_states_at(&compiled[0], &context, seconds * 2.0, seconds);
        for actor in sample["actors"].as_array().expect("native actors") {
            let name = actor["name"].as_str().expect("native name");
            if !matches!(name, "Runner" | "TV") {
                continue;
            }
            let index = compiled[0]
                .overlays
                .iter()
                .position(|overlay| overlay.name.as_deref() == Some(name))
                .expect("queued reset actor");
            for (axis, actual) in [states[index].x, states[index].y].into_iter().enumerate() {
                let expected =
                    value_f32(actor["current"]["position"].get(axis)).expect("native position");
                assert!(
                    (actual - expected).abs() <= 0.75,
                    "{name} at {seconds}, axis {axis}: {actual} vs native {expected}"
                );
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 484, "retain every native reset frame and axis");
    assert!(
        native["samples"][263]["actors"][1]["current"]["position"][0]
            .as_f64()
            .expect("position before reset")
            < 0.0
    );
    assert_eq!(
        native["samples"][264]["actors"][1]["current"]["position"][0],
        1120.0
    );
    assert_eq!(
        native["samples"][264]["actors"][2]["current"]["position"][0],
        832.0
    );
    eprintln!("queued reset: {checks} native position checks");
}

#[test]
fn stopped_position_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/stop-position.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/stop-position.sm"),
    );
    // Orthographic screen bounds cannot expose an incorrect stopped Z value.
    // Check native world depth through the same compiled playback path.
    let drawable_map = projected_drawable_map(&trace, &compiled);
    let mut depths = 0;
    for track in &trace.projected_vertex_tracks {
        let &(layer, index) = drawable_map.get(&track.actor).unwrap();
        for sample in &track.samples {
            let beat = value_f32(sample.get(0)).unwrap();
            let seconds = value_f32(sample.get(1)).unwrap();
            let states = compiled_overlay_states_at(&compiled[layer], &context, beat, seconds);
            let actual = compiled_world_vertices(states[index], track.texture_size);
            for (corner, vertex) in sample[4].as_array().unwrap().iter().enumerate() {
                let expected = value_f32(vertex.get(2)).unwrap();
                assert!(
                    (actual[corner][2] - expected).abs() <= 0.002,
                    "{} world Z at beat {beat}: {} vs {expected}",
                    track.actor,
                    actual[corner][2]
                );
                depths += 1;
            }
        }
    }
    assert_eq!(depths, 60);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Stop position"));
    assert_eq!(parity.checks(), 85);
    parity.assert_complete("Stop position");
}

#[test]
fn zoom_axis_fit_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/zoom-axis-fit.json.zst"),
    );
    let simfile = root.join("tests/fixtures/song-lua/zoom-axis-fit.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    let native: Value = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-song-lua-micro/zoom-axis-fit-native.json"))
            .unwrap(),
    )
    .unwrap();
    let mut checks = 0;
    for sample in native["samples"].as_array().unwrap() {
        let seconds = sample["time"].as_f64().unwrap() as f32;
        let states = compiled_overlay_states_at(&compiled[0], &context, seconds * 2.0, seconds);
        for actor in sample["actors"].as_array().unwrap().iter().skip(1) {
            let index = compiled[0]
                .overlays
                .iter()
                .position(|overlay| overlay.name.as_deref() == actor["name"].as_str())
                .unwrap();
            let size = [
                actor["size"][0].as_f64().unwrap() as f32,
                actor["size"][1].as_f64().unwrap() as f32,
            ];
            let corners = compiled_world_vertices(states[index], size);
            for corner in 0..4 {
                let expected = &actor["draws"][0]["vertices"][[0, 3, 2, 1][corner]]["screen"];
                for axis in 0..2 {
                    let expected = expected[axis].as_f64().unwrap() as f32;
                    assert!(
                        (expected - corners[corner][axis]).abs() <= 0.002,
                        "{} at {seconds} corner {corner} axis {axis}: {expected} vs {:?}",
                        actor["name"],
                        corners[corner]
                    );
                    checks += 1;
                }
            }
        }
    }
    assert_eq!(checks, 120);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Zoom axis fit"));
    assert!(parity.checks() > 100);
    parity.assert_complete("Zoom axis fit");
}

#[test]
fn nested_global_probes_match_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, checks) in [("global-probe", 767), ("global-probe-sibling", 759)] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json.zst"
        )));
        let simfile = root.join(format!("crates/deadsync-song-lua/tests/fixtures/{name}.sm"));
        let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        assert_eq!(parity.checks(), checks);
        parity.assert_complete(name);
    }
}

#[test]
fn recurring_initial_delay_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["recurring-delay", "recurring-delay-callback"] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json.zst"
        )));
        let simfile = root.join(format!("crates/deadsync-song-lua/tests/fixtures/{name}.sm"));
        let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        assert_eq!(parity.checks(), 257);
        parity.assert_complete(name);
    }
}

#[test]
fn recurring_follow_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["recurring-follow", "recurring-follow-offset"] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json.zst"
        )));
        let simfile = root.join(format!("crates/deadsync-song-lua/tests/fixtures/{name}.sm"));
        let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        assert_eq!(parity.checks(), 2070);
        parity.assert_complete(name);
    }
}

#[test]
fn finite_queue_controls_match_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["queue-control", "queue-recurring", "queue-vibration"] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json"
        )));
        let simfile = root.join(format!("crates/deadsync-song-lua/tests/fixtures/{name}.sm"));
        let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
        if name == "queue-vibration" {
            let actor = compiled[primary]
                .overlays
                .iter()
                .position(|actor| actor.name.as_deref() == Some("Cala"))
                .expect("native effect actor");
            let vibration = compiled[primary]
                .overlay_updates
                .iter()
                .find(|track| {
                    track.overlay_index == actor
                        && track.target == SongLuaOverlayUpdateTarget::Vibrate
                })
                .expect("runtime vibration track");
            for (command, enabled) in [("MainCommand", true), ("DoneCommand", false)] {
                let native = trace
                    .command_tracks
                    .iter()
                    .find(|track| track.command == command)
                    .and_then(|track| track.runs.first())
                    .and_then(|run| run.1)
                    .expect("native effect dispatch");
                let actual = vibration
                    .samples
                    .iter()
                    .find(|sample| sample.value == SongLuaOverlayUpdateValue::Bool(enabled))
                    .expect("compiled effect transition")
                    .time;
                let native = overlay_update_time(
                    &context,
                    compiled[primary].overlay_update_unit,
                    native,
                    song_elapsed_seconds_at(native, &context),
                );
                assert!(
                    (native - actual).abs() < 0.0001,
                    "{command}: {native} vs {actual}"
                );
            }
        }
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(&trace.title));
        parity.assert_complete(&trace.title);
    }
}

#[test]
#[ignore = "compiles the complete Cuphead song-Lua runtime at 60 Hz"]
fn cuphead_stateful_fire_message_matches_itgmania() {
    crate::paths::init();
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

    let mut parity = Parity::default();
    compare_commands(&trace, &compiled, primary_index, &mut parity);
    parity.assert_complete("Cuphead message-command");

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
                    && (299.0..=301.0).contains(&sample.time))
                .then_some(sample.time)
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
    crate::paths::init();
    let fixture_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/itgmania-song-lua");
    let manifest: SemanticManifest = serde_json::from_slice(
        &fs::read(fixture_root.join(SEMANTIC_MANIFEST))
            .expect("missing semantic song Lua fixture manifest"),
    )
    .expect("invalid semantic song Lua fixture manifest");

    assert_eq!(manifest.itgmania.execution, "embedded_bundled_lua");
    assert!(!manifest.itgmania.launches_executable);
    assert_eq!(manifest.simfiles.len(), 46);
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
fn itl_unlock_fixture_contexts() {
    crate::paths::init();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let flip69 = read_trace_file(&root.join(FLIP69_TRACE));
    assert_eq!(flip69.style, "double");
    assert_eq!(flip69.enabled_players, Some([true, false]));
    assert_eq!(flip69.trace_until_beat, 288.0);
    let zoom_spline = flip69
        .external_actors
        .iter()
        .find(|actor| {
            actor.path == "ScreenGameplay/PlayerP1/NoteField/Column8/GetZoomHandler/GetSpline"
        })
        .expect("flip69 must build the eighth-column multitap zoom spline");
    assert!(
        flip69.operation_tracks.iter().any(|track| {
            track.actor == zoom_spline.id
                && track.operation == "Spline.SetPoint"
                && track
                    .samples
                    .iter()
                    .any(|sample| sample.3.get(1) == Some(&serde_json::json!([-1, -1, -1])))
        }),
        "multitap regions must hide the original notes"
    );

    let riddle = read_trace_file(&root.join(RIDDLE_DOUBLE_TRACE));
    assert_eq!(riddle.style, "double");
    assert_eq!(riddle.enabled_players, Some([true, false]));
    assert_eq!(riddle.trace_until_beat, 300.5);
    assert_eq!(
        native_player_option_value_at(&riddle, 1, "PlayerOptions.Flip", 0.6),
        Some(1.0),
        "Riddle must execute its double-specific flip branch"
    );
    let handler = riddle
        .external_actors
        .iter()
        .find(|actor| actor.path == "ScreenGameplay/PlayerP1/NoteField/Column8/GetPosHandler")
        .expect("Riddle must exercise the eighth column");
    assert!(riddle.operation_tracks.iter().any(|track| {
        track.actor == handler.id
            && track.operation == "Spline.SetSplineMode"
            && track.samples.iter().any(|sample| {
                sample.3.first().and_then(Value::as_str) == Some("NoteColumnSplineMode_Disabled")
            })
    }));

    let cosmic = read_trace_file(&root.join(COSMIC_TRACE));
    assert_eq!(cosmic.style, "single");
    assert_eq!(cosmic.enabled_players, Some([true, true]));
    assert_eq!(cosmic.trace_until_beat, 348.0);
    let sprite = cosmic
        .projected_vertex_tracks
        .iter()
        .find(|track| track.texture.ends_with("_static 4x1.png"))
        .expect("CO5M1C static sprite trace");
    assert_eq!(
        sprite.texture_size,
        [480.0, 480.0],
        "use a frame, not the full sprite sheet"
    );
    let infinities = sprite
        .samples
        .iter()
        .filter_map(|sample| sample.get(3).and_then(projected_alpha))
        .filter(|alpha| alpha.is_infinite())
        .collect::<Vec<_>>();
    assert_eq!(infinities, [f32::INFINITY, f32::NEG_INFINITY]);
    let visible = sprite
        .samples
        .iter()
        .find(|sample| {
            value_f32(sample.get(0)).is_some_and(|beat| (71.2..71.3).contains(&beat))
                && value_f32(sample.get(3)) == Some(0.3)
        })
        .expect("CO5M1C static burst");
    let vertices = native_screen_vertices(visible.as_array().expect("projected sample"))
        .expect("static sprite corners");
    assert_eq!(vertex_bounds(&vertices), [0.0, -187.0, 854.0, 667.0]);
}

#[test]
fn brogamer_fixture_hides_and_scales_target_until_its_effect() {
    crate::paths::init();
    let trace = read_trace_file(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BROGAMER_TRACE));
    let target = trace
        .actor_definitions
        .iter()
        .find(|definition| definition.name.as_deref() == Some("TargetP1"))
        .expect("BroGamer fixture has no TargetP1 definition");
    let track = trace
        .projected_vertex_tracks
        .iter()
        .find(|track| track.definition_id.as_deref() == Some(target.id.as_str()))
        .expect("BroGamer fixture has no projected TargetP1 geometry");
    let first = track
        .samples
        .first()
        .and_then(Value::as_array)
        .expect("BroGamer target has no initial geometry sample");
    assert_eq!(value_f32(first.first()), Some(0.0));
    assert_eq!(first.get(2).and_then(Value::as_bool), Some(false));
    assert!(
        native_screen_vertices(first)
            .expect("BroGamer initial target sample has invalid vertices")
            .is_empty()
    );

    let visible = track
        .samples
        .iter()
        .filter_map(Value::as_array)
        .find(|sample| sample.get(2).and_then(Value::as_bool) == Some(true))
        .expect("BroGamer target never becomes visible");
    let beat = value_f32(visible.first()).expect("BroGamer target sample has no numeric beat");
    assert!(
        (304.0..305.0).contains(&beat),
        "target appears at beat {beat}"
    );
    let vertices =
        native_screen_vertices(visible).expect("BroGamer target sample has invalid vertices");
    let center = vertex_center(&vertices);
    assert!((center[0] - 273.0).abs() < 0.01, "target x: {center:?}");
    assert!((center[1] - 125.0).abs() < 0.01, "target y: {center:?}");
    let edge = (vertices[1][0] - vertices[0][0]).hypot(vertices[1][1] - vertices[0][1]);
    assert!(
        (70.0..135.0).contains(&edge),
        "target rendered at raw or otherwise incorrect size: {edge:.3}px"
    );
}

#[test]
fn brogamer_dizzy_and_confusion_do_not_leak_between_authored_windows() {
    crate::paths::init();
    let trace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BROGAMER_TRACE);
    let trace = read_trace_file(&trace_path);
    let (compiled, primary_index, context) = compile_trace_song(&trace);
    let compiled = &compiled[primary_index];
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: context.song_timing_bpms.clone(),
            ..deadsync_rules::timing::TimingSegments::default()
        },
        &[],
    );
    let constants = deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
        compiled, &timing, 0, 0.0,
    );
    let (eases, unsupported) = deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
        compiled, &timing, 0, 0.0, &constants,
    );
    assert_eq!(unsupported, 0);
    // Resets may be sampled targets rather than constant windows. Check the
    // resulting gameplay state at quiet and active native checkpoints below.
    let mut runtime = deadsync_gameplay::GameplayAttackRuntimeState::new(
        [constants, Vec::new()],
        [eases, Vec::new()],
    );
    let mut transform = deadsync_gameplay::SongLuaPlayerTransform::default();
    let mut previous_second = timing.get_time_for_beat(0.0);
    let mut samples = HashMap::new();
    for quarter in 0..=(440 * 4) {
        let beat = quarter as f32 * 0.25;
        let now = timing.get_time_for_beat(beat);
        if let Some(next) = runtime.refresh_player(
            0,
            now,
            (now - previous_second).max(0.0),
            deadsync_gameplay::AppearanceEffects::default(),
            deadsync_gameplay::AttackBaseEffects::default,
            transform,
        ) {
            transform = next;
        }
        if matches!(
            quarter,
            48 | 80 | 140 | 148 | 400 | 1180 | 1200 | 1236 | 1620
        ) {
            samples.insert(
                quarter,
                (
                    runtime.visual[0].dizzy.unwrap_or(0.0),
                    runtime.visual[0].confusion.unwrap_or(0.0),
                    runtime.visual[0].confusion_offset.unwrap_or(0.0),
                ),
            );
        }
        previous_second = now;
    }

    for quarter in [48, 80, 148, 400, 1180, 1236, 1620] {
        let beat = quarter as f32 * 0.25;
        let native_dizzy = native_player_option_value_at(&trace, 1, "PlayerOptions.Dizzy", beat)
            .expect("BroGamer fixture has no P1 Dizzy state at a quiet checkpoint");
        assert!(
            native_dizzy.abs() <= EPSILON,
            "BroGamer fixture expected Dizzy to be idle at beat {beat:.2}, got {native_dizzy:.4}",
        );
        let (dizzy, confusion, confusion_offset) = samples[&quarter];
        assert!(
            dizzy.abs() <= EPSILON
                && confusion.abs() <= EPSILON
                && confusion_offset.abs() <= EPSILON,
            "BroGamer rotation leaked at beat {:.2}: dizzy={dizzy:.4}, confusion={confusion:.4}, confusionoffset={confusion_offset:.4}",
            beat,
        );
    }
    for (quarter, minimum) in [(140, 4.8), (1200, 0.8)] {
        let beat = quarter as f32 * 0.25;
        let native_dizzy = native_player_option_value_at(&trace, 1, "PlayerOptions.Dizzy", beat)
            .expect("BroGamer fixture has no P1 Dizzy state in an active window");
        let actual_dizzy = samples[&quarter].0;
        assert!(native_dizzy >= minimum && actual_dizzy >= minimum);
        assert!(
            (actual_dizzy - native_dizzy).abs() <= 0.1,
            "BroGamer Dizzy differs at beat {beat:.2}: ITGmania={native_dizzy:.4}, DeadSync={actual_dizzy:.4}",
        );
    }
}

#[test]
fn cuphead_fixture_captures_queued_boss_spawns() {
    crate::paths::init();
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
    crate::paths::init();
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
fn bank_account_complete_semantics_match_itgmania() {
    crate::paths::init();
    let trace = read_trace_file(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "tests/fixtures/itgmania-song-lua-selected/Bank Account/Bank Account.ssc.semantic.json",
    ));
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert_eq!(
        parity.checks(),
        1160,
        "proxy, modifier and runtime timeline coverage must remain complete"
    );
    parity.assert_complete("Bank Account complete semantics");
}

#[test]
fn delightful_day_movie_and_hidden_layers_match_itgmania() {
    crate::paths::init();
    let trace = read_trace_file(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "tests/fixtures/itgmania-song-lua-selected/Delightful Day/Delightful Day.ssc.semantic.json",
    ));
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut complete = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut complete);
    assert_eq!(
        complete.checks(),
        1355,
        "movie geometry and runtime timeline must remain covered"
    );
    complete.assert_complete("Delightful Day complete rendering semantics");
    let layer = &compiled[primary];
    for (index, path) in [
        (0, "ScreenGameplay/Underlay"),
        (2, "ScreenGameplay/SongBackground"),
    ] {
        let native = trace
            .external_actors
            .iter()
            .find(|actor| actor.path == path)
            .expect("native screen layer");
        assert!(trace.operation_tracks.iter().any(|track| {
            track.actor == native.id
                && track.operation.ends_with(".visible")
                && track
                    .samples
                    .iter()
                    .any(|sample| sample.3.first() == Some(&Value::Bool(false)))
        }));
        assert!(
            !layer.screen_layers[index].initial_state.visible,
            "{path} must be hidden"
        );
    }
    let native_overlay = trace
        .external_actors
        .iter()
        .find(|actor| actor.path == "ScreenGameplay/Overlay")
        .expect("native Overlay");
    let fade = trace
        .tween_tracks
        .iter()
        .find(|track| {
            track.actor == native_overlay.id
                && track.kind == "tween"
                && track.easing.as_deref() == Some("smooth")
        })
        .expect("native HUD fade");
    assert!(
        layer.screen_layers[1]
            .message_commands
            .iter()
            .flat_map(|command| &command.blocks)
            .any(|block| block.duration == fade.segments[0].duration
                && block.easing.as_deref() == Some("smooth")
                && block.delta.diffuse.is_some_and(|color| color[3] == 0.0))
    );
    for player in 0..2 {
        assert!(layer.overlays.iter().any(
            |actor| matches!(actor.kind, SongLuaOverlayKind::ActorProxy {
            target: deadsync_assets::song_lua::SongLuaProxyTarget::Score { player_index }
        } if player_index == player)
        ));
    }
    let mut parity = Parity::default();
    compare_player_proxy_sources(&trace, &compiled, &mut parity);
    assert_eq!(
        parity.checks(),
        4,
        "both players need a target and an opaque source"
    );
    parity.assert_complete("Delightful Day player proxy sources");
    let mut faded = compiled;
    faded[0].player_actors[0].initial_state.diffuse[3] = 0.0;
    let mut regression = Parity::default();
    compare_player_proxy_sources(&trace, &faded, &mut regression);
    assert_eq!(
        regression.gaps.len(),
        1,
        "the report must detect invisible proxy sources"
    );
}

#[test]
fn step_your_game_up_critical_render_states_match_itgmania() {
    crate::paths::init();
    let trace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(STEP_YOUR_GAME_UP_TRACE);
    let trace = read_trace_file(&trace_path);
    let (compiled, _, context) = compile_trace_song(&trace);
    let mut parity = Parity::default();
    compare_update_render_values(&trace, &compiled, &context, &mut parity);
    let critical = parity
        .gaps
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
    assert_step_player_proxy_and_projection(&trace, &compiled, &context);
}

fn assert_step_player_proxy_and_projection(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
) {
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
                collect_native_overlay_definitions(&trace, root, &definitions, &mut native);
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
                context,
                layer,
                overlay_index,
                SongLuaOverlayUpdateTarget::Visible,
                hidden.0 as f32,
                song_elapsed_seconds_at(hidden.0 as f32, context),
            ),
            Some(SongLuaOverlayUpdateValue::Bool(false))
        );
        assert_eq!(
            compiled_update_value_at(
                context,
                layer,
                overlay_index,
                SongLuaOverlayUpdateTarget::Visible,
                restored.0 as f32,
                song_elapsed_seconds_at(restored.0 as f32, context),
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
fn delayed_actor_pool_is_ready_for_queued_updates() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/delayed-pool.json"));
    let simfile = root.join("crates/deadsync-song-lua/tests/fixtures/delayed-pool/pool.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    assert_eq!(parity.checks(), 63);
    parity.assert_complete("delayed actor pool");
    for (second, visible) in [(0.0, false), (1.1, true), (1.5, false)] {
        let states = compiled_local_states_at(&compiled[primary], &context, second, second);
        for name in ["First", "Second"] {
            let index = compiled[primary]
                .overlays
                .iter()
                .position(|actor| actor.name.as_deref() == Some(name))
                .expect("pooled actor");
            assert_eq!(states[index].visible, visible, "{name} at {second}");
            if second == 1.1 {
                assert!(states[index].diffuse[3] > 0.0);
            }
            if second == 1.5 {
                assert_eq!(states[index].diffuse[3], 0.0);
            }
        }
    }
}

#[test]
fn queued_startup_preserves_initial_overlay_state() {
    crate::paths::init();
    let temp = tempfile::tempdir().expect("create song directory");
    let song_dir = temp.path();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local bg, white, black
return Def.ActorFrame{
    OnCommand=function(self) self:queuecommand("Ready") end,
    ReadyCommand=function()
        bg:xy(427, 240):zoomto(2562, 1440)
        white:diffusealpha(0)
        black:diffusealpha(0)
    end,
    Def.Quad{
        Name="BG",
        InitCommand=function(self) bg = self end,
        OnCommand=function(self) self:x(7):queuecommand("ChildReady") end,
        ChildReadyCommand=function(self) self:x(450) end,
    },
    Def.Quad{Name="White", InitCommand=function(self) white = self end},
    Def.Quad{Name="Black", InitCommand=function(self) black = self end},
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Queued Startup");
    context.song_display_bpms = [120.0, 120.0];
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .unwrap()
        .remove(0);
    let event = compiled
        .messages
        .first()
        .expect("queued setup must be scheduled");
    assert_eq!(compiled.messages.len(), 1);
    assert!((event.beat - 1.0 / 30.0).abs() < 0.000_001);
    for name in ["BG", "White", "Black"] {
        let actor = compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some(name))
            .unwrap();
        assert_eq!(actor.initial_state.diffuse[3], 1.0);
        assert_eq!(actor.initial_state.size, None);
        let command = actor
            .message_commands
            .iter()
            .find(|command| command.message == event.message)
            .unwrap();
        let ready = overlay_state_after_blocks(actor.initial_state, &command.blocks, 0.0);
        if name == "BG" {
            assert_eq!(actor.initial_state.x, 7.0);
            assert_eq!([ready.x, ready.y], [450.0, 240.0]);
            assert_eq!([ready.zoom_x, ready.zoom_y], [2562.0, 1440.0]);
        } else {
            assert_eq!(ready.diffuse[3], 0.0);
        }
    }
}

#[test]
fn queued_tail_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, checks) in [("parent-rotation-cycle", 543), ("zero-message-size", 498)] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json.zst"
        )));
        let simfile = root.join(format!("tests/fixtures/song-lua/{name}.sm"));
        let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        assert_eq!(parity.checks(), checks);
        parity.assert_complete(name);
    }
}

#[test]
fn sprite_load_cycle_matches_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/sprite-load-cycle.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/sprite-load-cycle.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Sprite Load Cycle"));
    assert_eq!(parity.checks(), 23);
    parity.assert_complete("Sprite Load Cycle");
}

#[test]
fn recurring_tail_keeps_boundary_frame() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/repeat-boundary.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/repeat-boundary.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Repeat Boundary"));
    if !parity.gaps.is_empty() {
        for track in &compiled[primary].overlay_updates {
            if track.target == SongLuaOverlayUpdateTarget::RotationZ {
                eprintln!(
                    "boundary rotation samples: {:?}",
                    track
                        .samples
                        .iter()
                        .filter(|sample| (77.9..78.2).contains(&sample.time))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    assert_eq!(parity.checks(), 4915);
    parity.assert_complete("Repeat Boundary");
}

#[test]
fn foreground_comments_keep_native_layers() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/fg-comment-chain.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/fg-comment-chain.sm"),
    );
    assert_eq!(compiled.len(), 2);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Foreground Comment Chain"));
    assert_eq!(parity.checks(), 41);
    parity.assert_complete("Foreground Comment Chain");
}

#[test]
fn queued_visibility_matches_native_actor_updates() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let native: Value = serde_json::from_slice(
        &fs::read(
            root.join("tests/fixtures/itgmania-song-lua-micro/queued-visibility-native.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Queued Visibility");
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    context.music_length_seconds = 0.2;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(
        &[song_dir.join("queued-visibility.lua").as_path()],
        0,
        &context,
    )
    .unwrap();
    let mut checked = 0;
    for sample in native["samples"].as_array().unwrap() {
        let second = value_f32(sample.get("time")).unwrap();
        let states = compiled_local_states_at(&compiled[0], &context, second, second);
        for actor in sample["actors"].as_array().unwrap().iter().skip(1) {
            let index = compiled[0]
                .overlays
                .iter()
                .position(|overlay| overlay.name.as_deref() == actor["name"].as_str())
                .unwrap();
            assert_eq!(
                states[index].visible,
                actor["visible"].as_bool().unwrap(),
                "{} at {second}",
                actor["name"]
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 35);
}

#[test]
fn pulse_native_draws() {
    assert_eq!(native_effect_draws("pulse-body"), (7166, 160));
}

#[test]
fn motion_native_draws() {
    assert_eq!(native_effect_draws("motion-body"), (18674, 6292));
}

fn native_effect_draws(stem: &str) -> (usize, usize) {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song-lua");
    let trace = read_trace_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "tests/fixtures/itgmania-song-lua-micro/{stem}.json.zst"
    )));
    let (compiled, primary, context) =
        compile_trace_song_at(&trace, &dir.join(format!("{stem}.sm")));
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary(stem));
    parity.assert_complete(stem);
    let native_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "tests/fixtures/itgmania-song-lua-micro/{stem}-native.json"
    ));
    let native: Value = if native_path.exists() {
        serde_json::from_reader(fs::File::open(native_path).unwrap()).unwrap()
    } else {
        serde_json::from_reader(
            zstd::stream::read::Decoder::new(
                fs::File::open(native_path.with_extension("json.zst")).unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(native["oracle"], "itgmania_native_actor_conformance");
    let mut composer = WholeSongComposer::new(&compiled[primary].overlays);
    let mut checked = 0;
    for sample in native["samples"].as_array().unwrap() {
        let second = sample["time"].as_f64().unwrap() as f32;
        let beat = second * 2.0;
        let states = compiled_overlay_states_at(&compiled[primary], &context, beat, second);
        for actor in sample["actors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["draws"].as_array().is_some_and(|v| !v.is_empty()))
        {
            let index = compiled[primary]
                .overlays
                .iter()
                .position(|a| a.name.as_deref() == actor["name"].as_str())
                .unwrap();
            let frame = composer.render_overlay(
                &compiled[primary].overlays,
                &states,
                index,
                [854.0, 480.0],
                second,
                beat,
            );
            let expected = actor["draws"][0]["vertices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    [
                        v["screen"][0].as_f64().unwrap() as f32,
                        v["screen"][1].as_f64().unwrap() as f32,
                    ]
                })
                .collect::<Vec<_>>();
            let actual = rendered_quad_corners(&frame, [854.0, 480.0]);
            assert_eq!(actual.len(), 6, "quad must emit both triangles");
            for vertex in &expected {
                assert!(
                    actual.iter().any(|got| (got[0] - vertex[0]).abs() <= 0.75
                        && (got[1] - vertex[1]).abs() <= 0.75),
                    "{} at {second}: missing native corner {vertex:?}, rendered {actual:?}",
                    actor["name"]
                );
                checked += 1;
            }
            for vertex in &actual {
                assert!(
                    expected
                        .iter()
                        .any(|e| (e[0] - vertex[0]).abs() <= 0.75
                            && (e[1] - vertex[1]).abs() <= 0.75),
                    "{} at {second}: {expected:?} vs {actual:?}",
                    actor["name"]
                );
            }
        }
    }
    (parity.checks(), checked)
}

fn rendered_quad_corners(
    frame: &deadlib_present::render::RenderFrame,
    screen: [f32; 2],
) -> Vec<[f32; 2]> {
    let mut actual = Vec::new();
    for op in &frame.ops {
        if let deadlib_present::render::DrawOp::Sprite(run) = op {
            let camera = deadsync_song_lua::playback::actor_conformance::matrix_rows(
                frame.cameras[usize::from(run.camera)],
            );
            for instance in &frame.sprite_instances[run.instance_start as usize..]
                [..run.instance_count as usize]
            {
                let [s, c] = instance.rot_sin_cos;
                let [os, oc] = instance.local_offset_rot_sin_cos;
                let [ox, oy] = instance.local_offset;
                // Read the instanced quad using the sprite vertex shader's pose.
                for [x, y] in [
                    [-0.5, -0.5],
                    [0.5, -0.5],
                    [0.5, 0.5],
                    [-0.5, -0.5],
                    [0.5, 0.5],
                    [-0.5, 0.5],
                ] {
                    let [x, y] = [x * instance.size[0], y * instance.size[1]];
                    let clip = deadsync_song_lua::playback::actor_conformance::project_world(
                        camera,
                        [
                            instance.center[0] + (c * x - s * y) + (oc * ox - os * oy),
                            instance.center[1] + (s * x + c * y) + (os * ox + oc * oy),
                            instance.center[2],
                            1.0,
                        ],
                    );
                    actual.push([
                        (clip[0] / clip[3] + 1.0) * screen[0] * 0.5,
                        (1.0 - clip[1] / clip[3]) * screen[1] * 0.5,
                    ]);
                }
            }
            continue;
        }
        let deadlib_present::render::DrawOp::TexturedMesh(run) = op else {
            panic!("unexpected draw")
        };
        let camera = frame.cameras[usize::from(run.camera)];
        for instance in
            &frame.tmesh_instances[run.instance_start as usize..][..run.instance_count as usize]
        {
            for vertex in frame.tmesh_geometries[run.geometry as usize]
                .vertices
                .iter()
            {
                let clip = deadsync_song_lua::playback::actor_conformance::project_world(
                    deadsync_song_lua::playback::actor_conformance::matrix_rows(
                        camera * instance.transform(),
                    ),
                    [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0],
                );
                actual.push([
                    (clip[0] / clip[3] + 1.0) * screen[0] * 0.5,
                    (1.0 - clip[1] / clip[3]) * screen[1] * 0.5,
                ]);
            }
        }
    }
    actual
}

#[test]
fn pulse_driver_native_draws() {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/pulse-driver.json.zst"));
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/pulse-driver.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("pulse driver"));
    assert_eq!(parity.checks(), 1178);
    parity.assert_complete("pulse driver");
    let layer = &compiled[primary];
    assert!(
        layer
            .overlays
            .iter()
            .any(|actor| actor.name.is_none() && matches!(actor.kind, SongLuaOverlayKind::Actor))
    );
    let body = layer
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Body"))
        .unwrap();
    let mut composer = WholeSongComposer::new(&layer.overlays);
    let mut checked = 0;
    for sample in &trace.projected_vertex_tracks[0].samples {
        let beat = value_f32(sample.get(0)).unwrap();
        let seconds = value_f32(sample.get(1)).unwrap();
        let states = compiled_overlay_states_at(layer, &context, beat, seconds);
        let frame = composer.render_overlay(
            &layer.overlays,
            &states,
            body,
            [854.0, 480.0],
            seconds,
            beat,
        );
        let actual = rendered_quad_corners(&frame, [854.0, 480.0]);
        assert_eq!(actual.len(), 6, "quad must emit both triangles");
        let expected = sample[6]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| [value_f32(v.get(0)).unwrap(), value_f32(v.get(1)).unwrap()])
            .collect::<Vec<_>>();
        for vertex in &expected {
            assert!(
                actual
                    .iter()
                    .any(|got| (got[0] - vertex[0]).abs() <= 0.75
                        && (got[1] - vertex[1]).abs() <= 0.75),
                "at {seconds}: missing native corner {vertex:?}, rendered {actual:?}"
            );
            checked += 1;
        }
        for vertex in &actual {
            assert!(
                expected
                    .iter()
                    .any(|got| (got[0] - vertex[0]).abs() <= 0.75
                        && (got[1] - vertex[1]).abs() <= 0.75),
                "at {seconds}: unexpected rendered vertex {vertex:?}"
            );
        }
    }
    eprintln!("pulse driver rendered {checked} native corners");
    assert_eq!(checked, 360);
}

#[test]
fn late_pulse_native_draws() {
    let (checks, corners) = native_effect_draws("late-pulse");
    eprintln!("late pulse: {checks} semantic checks, {corners} rendered native corners");
    assert_eq!((checks, corners), (2210, 3368));
}

#[test]
fn effect_switch_native_draws() {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    let screen = [854.0, 480.0];
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        screen[0], screen[1],
    ));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/effect-switch.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/effect-switch.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("effect switch"));
    parity.assert_complete("effect switch");
    let native: Value = serde_json::from_reader(
        zstd::stream::read::Decoder::new(
            fs::File::open(
                root.join("tests/fixtures/itgmania-song-lua-micro/effect-switch-native.json.zst"),
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let layer = &compiled[primary];
    let mut composer = WholeSongComposer::new(&layer.overlays);
    let (mut selectors, mut corners, mut colors) = (0, 0, 0);
    for sample in native["samples"].as_array().unwrap() {
        let seconds = sample["time"].as_f64().unwrap() as f32;
        let beat = seconds * 2.0;
        let states = compiled_overlay_states_at(layer, &context, beat, seconds);
        for actor in sample["actors"].as_array().unwrap() {
            let Some(index) = layer
                .overlays
                .iter()
                .position(|a| a.name.as_deref() == actor["name"].as_str())
            else {
                continue;
            };
            let mut local = compiled_command_state_at(&context, layer, index, beat, seconds);
            apply_runtime_updates(&context, layer, index, beat, seconds, &mut local);
            let mode = actor["effect"]["type"].as_str().unwrap();
            assert_eq!(
                local.vibrate,
                mode == "vibrate",
                "{} at {seconds}",
                actor["name"]
            );
            assert_eq!(
                local.rainbow,
                mode == "rainbow",
                "{} at {seconds}",
                actor["name"]
            );
            assert_eq!(
                local.effect_mode,
                deadsync_song_lua::parse_overlay_effect_mode(mode).unwrap_or(EffectMode::None),
                "{} at {seconds}",
                actor["name"]
            );
            selectors += 3;
            if actor["draws"].as_array().unwrap().is_empty()
                || states[index].vibrate
                || states[index]
                    .inherited_vibrate
                    .iter()
                    .any(|value| *value != 0.0)
            {
                continue;
            }
            let frame =
                composer.render_overlay(&layer.overlays, &states, index, screen, seconds, beat);
            let actual = rendered_quad_corners(&frame, screen);
            let vertices = actor["draws"][0]["vertices"].as_array().unwrap();
            assert_eq!(actual.len(), 6);
            for vertex in vertices {
                let expected = [
                    value_f32(vertex["screen"].get(0)).unwrap(),
                    value_f32(vertex["screen"].get(1)).unwrap(),
                ];
                assert!(
                    actual.iter().any(|v| (v[0] - expected[0]).abs() <= 0.75
                        && (v[1] - expected[1]).abs() <= 0.75),
                    "{} at {seconds}: {expected:?} vs {actual:?}",
                    actor["name"]
                );
                corners += 1;
            }
            for instance in &frame.sprite_instances {
                for (channel, actual) in instance.tint.iter().enumerate() {
                    let expected = vertices[0]["color"][channel].as_u64().unwrap() as f32 / 255.0;
                    // Native draw colors are stored as bytes; allow one quantization step.
                    assert!(
                        (actual - expected).abs() <= 1.0 / 255.0 + 0.00001,
                        "{} at {seconds} channel {channel}: {expected} vs {actual}",
                        actor["name"]
                    );
                    colors += 1;
                }
            }
        }
    }
    eprintln!(
        "effect switch: {} semantic, {selectors} selectors, {corners} corners, {colors} draw colors",
        parity.checks()
    );
    assert_eq!(
        (parity.checks(), selectors, corners, colors),
        (950, 2169, 1448, 1448)
    );
}

fn native_color_draws(fixture: &str, end_seconds: f32) -> usize {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let screen = [854.0, 480.0];
    let mut context = SongLuaCompileContext::new(&song_dir, "Color inheritance");
    context.screen_width = screen[0];
    context.music_length_seconds = end_seconds;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(
        &[song_dir.join(format!("{fixture}.lua")).as_path()],
        0,
        &context,
    )
    .expect("compile color inheritance fixture");
    let native: Value = serde_json::from_reader(
        zstd::stream::read::Decoder::new(
            fs::File::open(root.join(format!(
                "tests/fixtures/itgmania-song-lua-micro/{fixture}-native.json.zst"
            )))
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(native["oracle"], "itgmania_native_actor_conformance");
    let layer = &compiled[0];
    let mut composer = WholeSongComposer::new(&layer.overlays);
    let (mut colors, mut failures) = (0, Vec::new());
    for sample in native["samples"].as_array().unwrap() {
        let seconds = sample["time"].as_f64().unwrap() as f32;
        let states = compiled_overlay_states_at(layer, &context, seconds, seconds);
        for actor in sample["actors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["kind"] == "sprite")
        {
            let name = actor["name"].as_str().unwrap();
            let index = layer
                .overlays
                .iter()
                .position(|a| a.name.as_deref() == Some(name))
                .unwrap();
            let frame =
                composer.render_overlay(&layer.overlays, &states, index, screen, seconds, seconds);
            let draws = actor["draws"].as_array().unwrap();
            assert_eq!(
                frame.sprite_instances.len(),
                draws.len(),
                "{name} at {seconds}"
            );
            for (instance, draw) in frame.sprite_instances.iter().zip(draws) {
                for vertex in draw["vertices"].as_array().unwrap() {
                    for (channel, actual) in instance.tint.iter().enumerate() {
                        let expected = vertex["color"][channel].as_u64().unwrap() as f32 / 255.0;
                        // Native draw colors are bytes; permit one quantization step.
                        if (actual - expected).abs() > 1.0 / 255.0 + 0.00001 && failures.len() < 12
                        {
                            failures.push(format!(
                                "{name} at {seconds} {} channel {channel}: {expected} vs {actual}",
                                draw["texture_mode"]
                            ));
                        }
                        colors += 1;
                    }
                }
            }
        }
    }
    eprintln!("{fixture}: {colors} native draw color channels");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    colors
}

#[test]
fn color_inheritance_native_draws() {
    assert_eq!(native_color_draws("color-inheritance", 2.0), 54656);
}

#[test]
fn late_colors_native_draws() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/late-colors.json.zst"));
    let (compiled, primary, context) =
        compile_trace_song_at(&trace, &root.join("tests/fixtures/song-lua/late-colors.sm"));
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("late colors"));
    parity.assert_complete("late colors");
    assert_eq!(native_color_draws("late-colors", 4.0), 57840);
}

fn native_actor_draws(name: &str, duration: f32) -> (usize, usize) {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&song_dir, "Fade clock");
    context.screen_width = 854.0;
    context.music_length_seconds = duration;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(
        &[song_dir.join(format!("{name}.lua")).as_path()],
        0,
        &context,
    )
    .expect("compile native fade fixture");
    let native: Value = serde_json::from_reader(
        zstd::stream::read::Decoder::new(
            fs::File::open(root.join(format!(
                "tests/fixtures/itgmania-song-lua-micro/{name}-native.json.zst"
            )))
            .expect("native fade capture"),
        )
        .expect("compressed native capture"),
    )
    .expect("native actor JSON");
    let layer = &compiled[0];
    let mut composer = WholeSongComposer::new(&layer.overlays);
    let (mut channels, mut draws, mut failures) = (0, 0, Vec::new());
    for sample in native["samples"].as_array().expect("native samples") {
        let seconds = sample["time"].as_f64().expect("sample time") as f32;
        let states = compiled_overlay_states_at(layer, &context, seconds, seconds);
        let composed = deadsync_song_lua::playback::actor_conformance::compose_overlay_states(
            &layer.overlays,
            &states,
            [854.0, 480.0],
            [seconds, seconds],
        );
        for actor in sample["actors"]
            .as_array()
            .expect("native actors")
            .iter()
            .filter(|actor| actor["kind"] == "sprite")
        {
            let name = actor["name"].as_str().expect("actor name");
            let index = layer
                .overlays
                .iter()
                .position(|overlay| overlay.name.as_deref() == Some(name))
                .expect("compiled native actor");
            for (field, actual) in [
                ("diffuse", composed[index].diffuse),
                ("glow", composed[index].glow),
            ] {
                let native_state = actor.get("effected").unwrap_or(&actor["current"]);
                let expected = if field == "diffuse" {
                    &native_state[field][0]
                } else {
                    &native_state[field]
                };
                for (channel, actual) in actual.iter().enumerate() {
                    let expected = expected[channel].as_f64().expect("native color") as f32;
                    if (actual - expected).abs() > f32::EPSILON && failures.len() < 16 {
                        failures.push(format!(
                            "{name} at {seconds} {field}[{channel}]: {expected} vs {actual}"
                        ));
                    }
                    channels += 1;
                }
            }
            let frame = composer.render_overlay(
                &layer.overlays,
                &states,
                index,
                [854.0, 480.0],
                seconds,
                seconds,
            );
            let expected = actor["draws"].as_array().expect("native draws").len();
            if frame.sprite_instances.len() != expected && failures.len() < 16 {
                failures.push(format!(
                    "{name} at {seconds}: native {expected} draws, DeadSync {}",
                    frame.sprite_instances.len()
                ));
            }
            draws += 1;
        }
    }
    eprintln!("{name}: {channels} native RGBA channels, {draws} draw-count checks");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    (channels, draws)
}

#[test]
fn fade_clock_matches_native() {
    assert_eq!(native_actor_draws("fade-clock", 4.0), (5784, 723));
}

#[test]
fn scheduled_fade_matches_native() {
    assert_eq!(native_actor_draws("scheduled-fade", 5.0), (9632, 1204));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/scheduled-fade.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/scheduled-fade.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("scheduled fade"));
    assert_eq!(
        parity.checks(),
        4706,
        "retain full scheduled-tween coverage"
    );
    parity.assert_complete("scheduled fade");
}

#[test]
fn short_glow_matches_native() {
    assert_eq!(native_actor_draws("short-glow", 8.0), (3848, 481));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/short-glow.json.zst"));
    let (compiled, primary, context) =
        compile_trace_song_at(&trace, &root.join("tests/fixtures/song-lua/short-glow.sm"));
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("short glow"));
    assert_eq!(
        parity.checks(),
        8601,
        "retain full short-period effect coverage"
    );
    parity.assert_complete("short glow");
}

#[test]
fn deferred_effects_match_native() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/deferred-effects.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/deferred-effects.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("deferred effects"));
    assert_eq!(
        parity.checks(),
        365,
        "retain every deferred-effect observation"
    );
    parity.assert_complete("deferred effects");
    let native: Value =
        serde_json::from_reader(
            zstd::stream::read::Decoder::new(
                fs::File::open(root.join(
                    "tests/fixtures/itgmania-song-lua-micro/deferred-effects-native.json.zst",
                ))
                .expect("native recurring queue capture"),
            )
            .expect("compressed queue capture"),
        )
        .expect("native queue JSON");
    let layer = &compiled[primary];
    let index = layer
        .overlays
        .iter()
        .position(|a| a.name.as_deref() == Some("Oscillator"))
        .expect("unrelated recurring actor");
    let samples = native["samples"].as_array().expect("native queue samples");
    assert_eq!(samples.len(), 421, "retain every native frame");
    for sample in samples {
        let time = sample["time"].as_f64().expect("native clock") as f32;
        let expected = sample["actors"][1]["current"]["rotation"][2]
            .as_f64()
            .expect("native angle") as f32;
        let actual = compiled_overlay_states_at(layer, &context, time, time)[index].rot_z_deg;
        assert!(
            (actual - expected).abs() <= f32::EPSILON,
            "unrelated queue at {time}: native {expected}, DeadSync {actual}"
        );
    }
    eprintln!("deferred effects: 421 native recurring-queue states");
}

#[test]
fn mawaru5_local_draw_colors_match_native() {
    crate::paths::init();
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/mawaru5-draw-colors.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("Mawaru5 local draw colors"));
    assert_eq!(
        parity.checks(),
        77718,
        "retain complete local chart coverage"
    );
    parity.assert_complete("Mawaru5 local draw colors");
}

#[test]
fn queued_messages_match_native() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/queued-broadcasts.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/queued-broadcasts.sm"),
    );
    let layer = &compiled[primary];
    let messages = layer
        .messages
        .iter()
        .filter(|event| matches!(event.message.as_str(), "Zero" | "Hit" | "Again" | "Never"))
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 3, "cancelled messages must never dispatch");
    for (event, (frame, name)) in
        messages
            .into_iter()
            .zip([(1, "Zero"), (13, "Hit"), (22, "Again")])
    {
        assert_eq!(event.message, name);
        assert!((event.beat - frame as f32 / 60.0).abs() <= f32::EPSILON);
    }
    let native: Value =
        serde_json::from_reader(
            zstd::stream::read::Decoder::new(
                fs::File::open(root.join(
                    "tests/fixtures/itgmania-song-lua-micro/queued-broadcasts-native.json.zst",
                ))
                .expect("native message queue capture"),
            )
            .expect("compressed native capture"),
        )
        .expect("native queue JSON");
    let samples = native["samples"].as_array().expect("native frame samples");
    assert_eq!(samples.len(), 181);
    let mut checks = 0;
    for sample in samples {
        let time = value_f32(sample.get("time")).expect("native frame time");
        let states = compiled_overlay_states_at(layer, &context, time, time);
        for actor in sample["actors"]
            .as_array()
            .expect("native actors")
            .iter()
            .skip(1)
        {
            let name = actor["name"].as_str().expect("native actor name");
            let index = layer
                .overlays
                .iter()
                .position(|a| a.name.as_deref() == Some(name))
                .expect("compiled queue actor");
            for (axis, actual) in [states[index].x, states[index].y].into_iter().enumerate() {
                let expected =
                    value_f32(actor["current"]["position"].get(axis)).expect("native position");
                assert!(
                    (actual - expected).abs() <= 0.0001,
                    "{name} at {time}, axis {axis}: native {expected}, DeadSync {actual}"
                );
                checks += 1;
            }
            for (channel, actual) in states[index].diffuse.into_iter().enumerate() {
                let expected =
                    value_f32(actor["current"]["diffuse"][0].get(channel)).expect("native diffuse");
                assert!(
                    (actual - expected).abs() <= 0.000001,
                    "{name} at {time}, channel {channel}: native {expected}, DeadSync {actual}"
                );
                checks += 1;
            }
        }
    }
    assert_eq!(
        checks, 4344,
        "retain every native frame, position and color"
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("queued messages"));
    assert_eq!(
        parity.checks(),
        259,
        "retain every queued-message observation"
    );
    parity.assert_complete("queued messages");
}

#[test]
fn unnamed_message_child_matches_native() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/unnamed-broadcast.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/unnamed-broadcast.sm"),
    );
    let layer = &compiled[primary];
    let family = layer
        .overlays
        .iter()
        .position(|a| a.name.as_deref() == Some("Family"))
        .expect("receiver family");
    let native: Value =
        serde_json::from_reader(
            zstd::stream::read::Decoder::new(
                fs::File::open(root.join(
                    "tests/fixtures/itgmania-song-lua-micro/unnamed-broadcast-native.json.zst",
                ))
                .expect("native unnamed lookup capture"),
            )
            .expect("compressed native capture"),
        )
        .expect("native lookup JSON");
    let samples = native["samples"].as_array().expect("native lookup frames");
    assert_eq!(samples.len(), 61);
    let mut checks = 0;
    for sample in samples {
        let time = value_f32(sample.get("time")).expect("native time");
        let states = compiled_overlay_states_at(layer, &context, time, time);
        for actor in sample["actors"]
            .as_array()
            .expect("native actors")
            .iter()
            .skip(2)
        {
            let name = actor["name"].as_str().expect("native child name");
            let index = layer
                .overlays
                .iter()
                .position(|a| {
                    a.parent_index == Some(family) && a.name.as_deref().unwrap_or("") == name
                })
                .expect("native child in compiler tree");
            for (channel, actual) in states[index].diffuse.into_iter().enumerate() {
                let expected = value_f32(actor["current"]["diffuse"][0].get(channel))
                    .expect("native child color");
                assert_eq!(
                    actual, expected,
                    "child {name:?} at {time}, channel {channel}"
                );
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 488);
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("unnamed message child"));
    assert_eq!(
        parity.checks(),
        51,
        "retain every unnamed-child observation"
    );
    parity.assert_complete("unnamed message child");
}

#[test]
fn shared_screen_translation_matches_native() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/screen-translation.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/screen-translation.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("shared screen translation"));
    assert_eq!(
        parity.checks(),
        212,
        "retain both cameras and the flat actor"
    );
    parity.assert_complete("shared screen translation");
}

#[test]
fn queued_child_cycles_match_native() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, checks) in [("queued-child-cycle", 686), ("queued-child-tween", 334)] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json.zst"
        )));
        let (compiled, primary, context) = compile_trace_song_at(
            &trace,
            &root.join(format!("tests/fixtures/song-lua/{name}.sm")),
        );
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        assert_eq!(parity.checks(), checks, "retain every native child frame");
        parity.assert_complete(name);
    }
}

#[test]
fn runtime_cycle_start_and_screen_camera_match_native() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, checks) in [("queued-cycle-start", 622), ("screen-camera-change", 40)] {
        let trace = read_trace_file(&root.join(format!(
            "tests/fixtures/itgmania-song-lua-micro/{name}.json.zst"
        )));
        let (compiled, primary, context) = compile_trace_song_at(
            &trace,
            &root.join(format!("tests/fixtures/song-lua/{name}.sm")),
        );
        let parity = compare_semantics(&trace, &compiled, primary, &context);
        eprintln!("{}", parity.summary(name));
        assert_eq!(parity.checks(), checks, "retain every native runtime frame");
        parity.assert_complete(name);
    }
}

#[test]
fn judgment_sheet_changes_match_native() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/judgment-sheet-changes.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song_at(
        &trace,
        &root.join("tests/fixtures/song-lua/judgment-sheet-changes.sm"),
    );
    let parity = compare_semantics(&trace, &compiled, primary, &context);
    eprintln!("{}", parity.summary("judgment sheet changes"));
    assert_eq!(
        parity.checks(),
        14,
        "retain both players and unsampled loads"
    );
    parity.assert_complete("judgment sheet changes");
    for player in &compiled[primary].player_actors {
        for (second, expected) in [
            (0.0, "Normal 2x6.png"),
            (0.075, "Fake 2x6.png"),
            (0.11, "Normal 2x6.png"),
        ] {
            let texture = deadsync_song_lua::judgment_texture_at(&player.judgment, second)
                .expect("baked resource selection for gameplay rendering");
            assert_eq!(texture.path.file_name().unwrap(), expected);
            assert_eq!(texture.frame_size, [64.0, 64.0]);
            assert_eq!((texture.frame_cols, texture.frame_rows), (2, 6));
        }
    }
}

#[test]
fn mawaru8_local_messages_match_native() {
    crate::paths::init();
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/mawaru8-queued-messages.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Mawaru8 local queued messages"));
    assert_eq!(
        parity.checks(),
        331138,
        "retain the complete corrected local trace"
    );
    parity.assert_complete("Mawaru8 local queued messages");
}

#[test]
fn warp_zone_whole_song_matches_native() {
    crate::paths::init();
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/warp-zone-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Warp Zone whole song"));
    parity.assert_complete("Warp Zone whole song");
    assert_eq!(parity.checks(), 9201, "retain every native observation");
    let mut missing = compiled.clone();
    for layer in &mut missing {
        layer.overlay_updates.clear();
    }
    let mut rejected = Parity::default();
    compare_projected_geometry(&trace, &missing, &context, &mut rejected);
    assert!(
        rejected.passed() < rejected.checks(),
        "missing portal updates must fail"
    );
}

#[test]
fn bad_apple_whole_song_matches_native() {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/bad-apple-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    runtime_modifiers::compare_player_frames(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Bad Apple whole song"));
    parity.assert_complete("Bad Apple whole song");
    assert_eq!(
        parity.checks(),
        275833,
        "retain every native observation and quiet frame"
    );
    let map = projected_drawable_map(&trace, &compiled);
    assert_eq!(
        trace.projected_vertex_tracks.len(),
        1,
        "the AVI must have native geometry"
    );
    let track = &trace.projected_vertex_tracks[0];
    assert_eq!(track.texture_size, [320.0, 240.0]);
    let &(layer, index) = map.get(&track.actor).expect("movie drawable");
    let SongLuaOverlayKind::Sprite { texture_key, .. } = &compiled[layer].overlays[index].kind
    else {
        panic!("movie must be a sprite");
    };
    // Gameplay's worker warmup supplies decoded movie dimensions. Compilation
    // alone does not register video pixels; exercise that production path here.
    warm_lua_movie(texture_key, (320, 240));
    let mut composer = WholeSongComposer::new(&compiled[layer].overlays);
    let screen = [context.screen_width, context.screen_height];
    for sample in &track.samples {
        let sample = sample.as_array().expect("movie sample");
        let beat = value_f32(sample.first()).expect("movie beat");
        let second = value_f32(sample.get(1)).expect("movie second");
        let states = compiled_overlay_states_at(&compiled[layer], &context, beat, second);
        let frame = composer.render_overlay(
            &compiled[layer].overlays,
            &states,
            index,
            screen,
            second,
            beat,
        );
        let actual = rendered_quad_corners(&frame, screen);
        let expected = sample[6].as_array().expect("movie corners");
        assert_eq!(expected.len(), 4);
        assert!(!actual.is_empty(), "movie must emit textured geometry");
        for corner in expected {
            let x = value_f32(corner.get(0)).expect("movie x");
            let y = value_f32(corner.get(1)).expect("movie y");
            assert!(
                actual
                    .iter()
                    .any(|got| (got[0] - x).abs() <= 0.75 && (got[1] - y).abs() <= 0.75),
                "missing movie corner [{x}, {y}], rendered {actual:?}"
            );
        }
    }
    assert_eq!(
        compiled
            .iter()
            .map(|layer| layer.boolean_writes.len())
            .sum::<usize>(),
        42
    );
    let mut missing = compiled.clone();
    missing[primary].boolean_writes.clear();
    let mut rejected = Parity::default();
    runtime_modifiers::compare_runtime_modifiers(&trace, &missing, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "missing boolean setter evidence must fail"
    );
    let mut wrong = compiled.clone();
    wrong[primary].boolean_writes[0].previous = true;
    let mut rejected = Parity::default();
    runtime_modifiers::compare_runtime_modifiers(&trace, &wrong, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "wrong previous-value returns must fail"
    );
    let mut missing = compiled.clone();
    if let SongLuaOverlayKind::Sprite {
        texture_key,
        textures,
        ..
    } = &mut missing[layer].overlays[index].kind
    {
        *texture_key = std::sync::Arc::from("missing-badapple.avi");
        *textures = Default::default();
    } else {
        panic!("native movie must be a Sprite");
    }
    let mut rejected = Parity::default();
    compare_sprite_textures(&trace, &missing, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "missing movie bindings must fail"
    );
}

#[test]
fn goodbye_whole_song_matches_native() {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/goodbye-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Goodbye whole song"));
    parity.assert_complete("Goodbye whole song");
    assert_eq!(parity.checks(), 118437, "retain every native observation");
    let map = projected_drawable_map(&trace, &compiled);
    let screen = [context.screen_width, context.screen_height];

    let mut composer = WholeSongComposer::new(&compiled[primary].overlays);
    let mut drawn_faces = 0;
    for track in trace.projected_vertex_tracks.iter().filter(|track| {
        map.get(&track.actor).is_some_and(|&(layer, index)| {
            matches!(
                compiled[layer].overlays[index].kind,
                SongLuaOverlayKind::Sprite { .. }
            )
        })
    }) {
        let on_screen = |sample: &&Value| {
            if sample[2].as_bool() != Some(true) {
                return false;
            }
            let sample = sample.as_array().expect("native face sample");
            let vertices = native_crop_corners(sample, screen);
            if vertices.is_empty() {
                return false;
            }
            let [left, top, right, bottom] = vertex_bounds(&vertices);
            right > 0.0 && bottom > 0.0 && left < screen[0] && top < screen[1]
        };
        let Some(first) = track.samples.iter().find(on_screen) else {
            continue;
        };
        let last = track
            .samples
            .iter()
            .rev()
            .find(on_screen)
            .expect("first on-screen sample exists");
        let &(layer, index) = map.get(&track.actor).expect("face has a drawable");
        assert_eq!(layer, primary);
        for sample in [first, last] {
            let sample = sample.as_array().expect("native face sample");
            let beat = value_f32(sample.first()).expect("native beat");
            let second = value_f32(sample.get(1)).expect("native seconds");
            let states = compiled_overlay_states_at(&compiled[layer], &context, beat, second);
            let frame = composer.render_overlay(
                &compiled[layer].overlays,
                &states,
                index,
                screen,
                second,
                beat,
            );
            let actual = rendered_quad_corners(&frame, screen);
            for op in &frame.ops {
                let deadlib_present::render::DrawOp::TexturedMesh(run) = op else {
                    continue;
                };
                for instance in &frame.tmesh_instances[run.instance_start as usize..]
                    [..run.instance_count as usize]
                {
                    let matrix = deadsync_song_lua::playback::actor_conformance::matrix_rows(
                        frame.cameras[usize::from(run.camera)] * instance.transform(),
                    );
                    for vertex in frame.tmesh_geometries[run.geometry as usize]
                        .vertices
                        .iter()
                    {
                        let clip = deadsync_song_lua::playback::actor_conformance::project_world(
                            matrix,
                            [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0],
                        );
                        assert!(
                            clip[2].abs() <= clip[3] + 0.0001,
                            "{} at {second}: visible face must stay within native orthographic depth, {clip:?}",
                            track.actor,
                        );
                    }
                }
            }
            let mut expected = native_crop_corners(sample, screen);
            let shadow = native_draw_shadow(sample).expect("native shadow snapshot");
            if value_f32(sample.get(3)).is_some_and(|alpha| alpha > 0.0)
                && (shadow[0] != 0.0 || shadow[1] != 0.0)
            {
                // Goodbye uses the orthographic screen camera. Sprite applies
                // these offsets in world coordinates, independently of zoom.
                assert_eq!(sample[7][0], 0);
                expected.extend(
                    expected
                        .clone()
                        .into_iter()
                        .map(|[x, y]| [x + shadow[0], y + shadow[1]]),
                );
            }
            assert!(
                !actual.is_empty(),
                "{} must emit a textured face at {second}",
                track.actor
            );
            for vertex in &expected {
                assert!(
                    actual.iter().any(|got| (got[0] - vertex[0]).abs() <= 0.75
                        && (got[1] - vertex[1]).abs() <= 0.75),
                    "{} at {second}: missing {vertex:?}, rendered {actual:?}",
                    track.actor
                );
            }
            for vertex in &actual {
                assert!(
                    expected.iter().any(|got| (got[0] - vertex[0]).abs() <= 0.75
                        && (got[1] - vertex[1]).abs() <= 0.75),
                    "{} at {second}: unexpected {vertex:?}, native {expected:?}",
                    track.actor
                );
            }
        }
        drawn_faces += 1;
    }
    assert_eq!(
        drawn_faces, 51,
        "all visible face sprites must use the production renderer"
    );
    let mut missing = compiled.clone();
    for track in trace.projected_vertex_tracks.iter().filter(|track| {
        map.get(&track.actor).is_some_and(|&(layer, index)| {
            matches!(
                compiled[layer].overlays[index].kind,
                SongLuaOverlayKind::Sprite { .. }
            )
        }) && track
            .samples
            .iter()
            .any(|sample| sample[2].as_bool() == Some(true))
    }) {
        let &(layer, index) = map.get(&track.actor).expect("aliased face");
        if let SongLuaOverlayKind::Sprite { textures, .. } =
            &mut missing[layer].overlays[index].kind
        {
            *textures = Default::default();
        }
    }
    let mut rejected = Parity::default();
    compare_sprite_textures(&trace, &missing, &context, &mut rejected);
    assert!(
        rejected.passed() < rejected.checks(),
        "missing texture aliases must fail"
    );
    let mut uncropped = compiled.clone();
    uncropped[primary]
        .overlay_updates
        .retain(|update| update.target != SongLuaOverlayUpdateTarget::CropTop);
    let mut rejected = Parity::default();
    compare_projected_geometry(&trace, &uncropped, &context, &mut rejected);
    assert!(!rejected.gaps.is_empty(), "missing crop updates must fail");
}

#[test]
fn and_drugs_whole_song_matches_native() {
    crate::paths::init();
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/and-drugs-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("And Drugs whole song"));
    parity.assert_complete("And Drugs whole song");
    assert_eq!(parity.checks(), 29680, "retain every native observation");
    let mut missing = compiled.clone();
    for layer in &mut missing {
        layer.eases.clear();
    }
    let mut rejected = Parity::default();
    runtime_modifiers::compare_runtime_modifiers(&trace, &missing, &context, &mut rejected);
    assert!(
        rejected.passed() < rejected.checks(),
        "missing modifier targets must fail"
    );
}

#[test]
fn igaku_whole_song_matches_native() {
    crate::paths::init();
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/igaku-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Igaku whole song"));
    parity.assert_complete("Igaku whole song");
    assert_eq!(
        parity.checks(),
        331752,
        "retain every native observation in its owning layer"
    );
    let mut missing = compiled.clone();
    for layer in &mut missing {
        layer.column_offsets.retain(|window| {
            window.target != deadsync_song_lua::SongLuaColumnTransformTarget::RotationZ
        });
    }
    let mut rejected = Parity::default();
    compare_column_splines(&trace, &missing, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "the audit rejects missing column rotations"
    );
}

#[test]
fn mawaru6_whole_song_matches_native() {
    crate::paths::init();
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/mawaru6-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Mawaru6 whole song"));
    assert_eq!(parity.checks(), 686877, "retain every native observation");
    parity.assert_complete("Mawaru6 whole song");
}

#[test]
fn mawaru7_whole_song_matches_native() {
    crate::paths::init();
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/mawaru7-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Mawaru7 whole song"));
    assert_eq!(parity.checks(), 874665, "retain every native observation");
    parity.assert_complete("Mawaru7 whole song");
}

fn compare_drawable_frames(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    parity.section("drawable update frames");
    let map = projected_drawable_map(trace, compiled);
    let mut cursors = vec![0; trace.projected_vertex_tracks.len()];
    for (frame, &(beat, second)) in trace.update_frames.iter().enumerate() {
        let states = compiled
            .iter()
            .map(|layer| compiled_local_states_at(layer, context, beat as f32, second as f32))
            .collect::<Vec<_>>();
        for (track, cursor) in trace.projected_vertex_tracks.iter().zip(&mut cursors) {
            let native = trace
                .runtime_actors
                .iter()
                .find(|actor| actor.id == track.actor)
                .expect("native drawable state");
            while native
                .render_state_samples
                .get(*cursor + 1)
                .is_some_and(|sample| sample.0 <= frame)
            {
                *cursor += 1;
            }
            let &(_, alpha, visible) = native
                .render_state_samples
                .get(*cursor)
                .expect("native initial drawable state");
            let &(layer, index) = map.get(&track.actor).expect("compiled drawable");
            let actual = states[layer][index];
            parity.check(
                alpha.is_some_and(|alpha| (alpha - actual.diffuse[3]).abs() <= EPSILON),
                || {
                    format!(
                        "{} frame {frame} beat {beat}: native alpha {alpha:?}, DeadSync {}",
                        track.actor, actual.diffuse[3]
                    )
                },
            );
            parity.check(visible == actual.visible, || {
                format!(
                    "{} frame {frame} beat {beat}: native visible {visible}, DeadSync {}",
                    track.actor, actual.visible
                )
            });
        }
    }
}

fn warm_lua_movie(key: &str, expected: (u32, u32)) {
    let prepared = deadsync_assets::dynamic_media::prepare_song_lua_video(Path::new(key), true);
    let deadsync_assets::dynamic_media::SongLuaVideoPrepResult::Ready(prepared) = prepared else {
        panic!("local movie preparation failed");
    };
    assert_eq!(prepared.key, key);
    let poster = prepared
        .poster
        .expect("local movie poster")
        .expect("requested poster");
    assert_eq!(poster.dimensions(), expected);
    deadlib_assets::register_texture_dims(&prepared.key, poster.width(), poster.height());
    deadsync_assets::dynamic_media::retire_video_player(prepared.player);
}

fn warm_lua_image(key: &str, expected: (u32, u32)) {
    let image = deadlib_assets::open_image_fallback(Path::new(key))
        .expect("decode local sprite using production image loader");
    assert_eq!((image.width(), image.height()), expected);
    deadlib_assets::register_texture_dims(key, image.width(), image.height());
}

fn native_crop_corners(sample: &[Value], screen: [f32; 2]) -> Vec<[f32; 2]> {
    let [left, right, top, bottom] = native_draw_crop(sample)
        .expect("native crop snapshot")
        .map(|value| value.clamp(0.0, 1.0));
    if left + right >= 1.0 || top + bottom >= 1.0 {
        return Vec::new();
    }
    let corners = sample[5].as_array().expect("native clip corners");
    // Sprite::DrawTexture crops local corners before the perspective
    // divide. Interpolate the native homogeneous corners on that plane.
    [
        [left, top],
        [1.0 - right, top],
        [1.0 - right, 1.0 - bottom],
        [left, 1.0 - bottom],
    ]
    .map(|[u, v]| {
        let clip: [f32; 4] = std::array::from_fn(|axis| {
            let corner =
                |index: usize| value_f32(corners[index].get(axis)).expect("native clip coordinate");
            (corner(0) * (1.0 - u) + corner(1) * u) * (1.0 - v)
                + (corner(3) * (1.0 - u) + corner(2) * u) * v
        });
        [
            (clip[0] / clip[3] + 1.0) * screen[0] * 0.5,
            (1.0 - clip[1] / clip[3]) * screen[1] * 0.5,
        ]
    })
    .to_vec()
}

fn compare_image_draws(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
) -> usize {
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    let map = projected_drawable_map(trace, compiled);
    let mut composers = compiled
        .iter()
        .map(|layer| WholeSongComposer::new(&layer.overlays))
        .collect::<Vec<_>>();
    let screen = [context.screen_width, context.screen_height];
    let mut checks = 0;
    for track in &trace.projected_vertex_tracks {
        let &(layer, index) = map.get(&track.actor).expect("image drawable");
        if !matches!(
            compiled[layer].overlays[index].kind,
            SongLuaOverlayKind::Sprite { .. } | SongLuaOverlayKind::AftSprite { .. }
        ) {
            continue;
        }
        for sample in &track.samples {
            let sample = sample.as_array().expect("image sample");
            if sample[2].as_bool() != Some(true) || value_f32(sample.get(3)).unwrap_or(0.0) <= 0.0 {
                continue;
            }
            let beat = value_f32(sample.first()).expect("image beat");
            let second = value_f32(sample.get(1)).expect("image second");
            let mut states = compiled_overlay_states_at(&compiled[layer], context, beat, second);
            // Native projected corners omit random vibration translation.
            // Its active magnitudes are audited separately; compose the same
            // nominal pose through the production image builder here.
            states[index].vibrate = false;
            states[index].inherited_vibrate = [0.0; 3];
            let frame = composers[layer].render_overlay(
                &compiled[layer].overlays,
                &states,
                index,
                screen,
                second,
                beat,
            );
            let actual = rendered_quad_corners(&frame, screen);
            let expected = native_crop_corners(sample, screen);
            let area = expected
                .iter()
                .zip(expected.iter().cycle().skip(1))
                .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
                .sum::<f32>();
            let outside = !expected.is_empty()
                && ((0..2).any(|axis| {
                    expected.iter().all(|point| point[axis] <= 0.0)
                        || expected.iter().all(|point| point[axis] >= screen[axis])
                }));
            let shadow = native_draw_shadow(sample).expect("native image shadow");
            let same_footprint = shadow[5] <= 0.0 || (shadow[0] == 0.0 && shadow[1] == 0.0);
            // The native geometry oracle retains off-screen planes. Production
            // composition may cull them; verify the absence of output as well.
            if expected.is_empty()
                || area == 0.0
                || (outside && same_footprint && frame.ops.is_empty())
            {
                assert!(
                    actual.is_empty(),
                    "{} beat {beat}: empty visible plane corners",
                    track.actor
                );
                assert!(
                    frame.sprite_instances.is_empty(),
                    "{} beat {beat}: empty visible plane sprite",
                    track.actor
                );
                assert!(
                    frame.ops.is_empty(),
                    "{} beat {beat}: empty visible plane draw",
                    track.actor
                );
                checks += 3;
                continue;
            }
            for [x, y] in expected {
                assert!(
                    actual
                        .iter()
                        .any(|got| (got[0] - x).abs() <= 0.75 && (got[1] - y).abs() <= 0.75),
                    "{} beat {beat}: missing corner [{x},{y}], actual {actual:?}",
                    track.actor
                );
                checks += 1;
            }
            assert_eq!(
                frame.sprite_instances.len(),
                1,
                "{} beat {beat} image draw",
                track.actor
            );
            if matches!(
                compiled[layer].overlays[index].kind,
                SongLuaOverlayKind::AftSprite { .. }
            ) {
                assert!(frame.ops.iter().all(|op| matches!(op,
                    deadlib_present::render::DrawOp::Sprite(run)
                        if deadlib_present::render::is_render_target_texture(run.texture_handle))));
            }
            for (channel, actual) in frame.sprite_instances[0].tint.iter().enumerate() {
                let expected = value_f32(sample[9].get(channel)).expect("image diffuse");
                assert!(
                    (expected - actual).abs() <= EPSILON,
                    "{} beat {beat} channel {channel}: native {expected}, DeadSync {actual}",
                    track.actor
                );
                checks += 1;
            }
        }
    }
    checks
}

#[test]
fn save_tears_whole_native() {
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/save-your-tears-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    assert_eq!(
        trace.projected_vertex_tracks.len(),
        5,
        "retain JPEG background geometry"
    );
    assert_eq!(trace.update_frames.len(), 10252, "retain the entire song");
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    compare_drawable_frames(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Save Your Tears whole song"));
    parity.assert_complete("Save Your Tears whole song");
    assert_eq!(
        parity.checks(),
        126766,
        "retain every observation and quiet frame"
    );
    let map = projected_drawable_map(&trace, &compiled);
    let movie = trace
        .projected_vertex_tracks
        .iter()
        .find(|track| track.texture.ends_with(".mp4"))
        .expect("local rain movie");
    let &(layer, index) = map.get(&movie.actor).expect("rain sprite");
    let SongLuaOverlayKind::Sprite { texture_key, .. } = &compiled[layer].overlays[index].kind
    else {
        panic!("rain movie must be a sprite");
    };
    warm_lua_movie(texture_key, (1280, 720));
    // Gameplay warms still-image metadata with decoded pixels before building
    // actors. The headless composer also needs that resource handoff.
    let background = trace
        .projected_vertex_tracks
        .iter()
        .find(|track| track.texture.ends_with(".jpg"))
        .expect("local JPEG background");
    let &(bg_layer, bg_index) = map.get(&background.actor).expect("background sprite");
    let SongLuaOverlayKind::Sprite { texture_key, .. } =
        &compiled[bg_layer].overlays[bg_index].kind
    else {
        panic!("JPEG background must be a sprite");
    };
    warm_lua_image(texture_key, (3224, 2240));
    let draws = compare_image_draws(&trace, &compiled, &context);
    eprintln!("Save Your Tears actual image draw checks: {draws}");
    assert_eq!(
        draws, 6808,
        "retain all visible movie and JPEG observations"
    );
    let mut wrong = compiled.clone();
    let track = wrong[layer]
        .overlay_updates
        .iter_mut()
        .find(|track| {
            track.overlay_index == index && track.target == SongLuaOverlayUpdateTarget::Diffuse
        })
        .expect("retargeted rain fade track");
    for sample in &mut track.samples {
        if let SongLuaOverlayUpdateValue::Vec4(value) = &mut sample.value {
            value[3] = 0.95;
        }
    }
    let mut rejected = Parity::default();
    compare_drawable_frames(&trace, &wrong, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "incorrect fade values must fail full-frame checks"
    );
}

#[test]
fn i_ai_whole_native() {
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/i-ai-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    assert_eq!(compiled.len(), 2, "retain background and foreground");
    assert_eq!(trace.projected_vertex_tracks.len(), 6);
    assert_eq!(trace.update_frames.len(), 7201);
    assert_eq!(trace.fixture_context.note_end_beat, Some(444.0));
    assert!(
        trace
            .fixture_context
            .specified_last_beat
            .is_some_and(|beat| beat > 444.0)
    );
    assert_eq!(trace.end_position.beat, Some(450.0));
    let tv = trace
        .projected_vertex_tracks
        .iter()
        .find(|track| track.texture.ends_with("TVTurnOff.png"))
        .expect("final TV effect");
    assert!(
        tv.samples
            .iter()
            .any(|sample| sample[0].as_f64().is_some_and(|beat| beat > 444.0)
                && sample[2] == true
                && sample[3] == 1)
    );
    let collapsed = tv
        .samples
        .iter()
        .find(|sample| {
            sample[0].as_f64().is_some_and(|beat| beat > 446.0)
                && sample[4]
                    .as_array()
                    .is_some_and(|corners| corners.iter().all(|corner| corner == &corners[0]))
        })
        .expect("retain completed final TV shutoff");
    assert!(
        trace.end_position.beat.unwrap() > value_f32(collapsed.get(0)).unwrap() + 3.0,
        "retain quiet frames after the final effect"
    );
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    compare_drawable_frames(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("I (Ai) whole song"));
    parity.assert_complete("I (Ai) whole song");
    assert_eq!(
        parity.checks(),
        241877,
        "retain every observation and quiet frame"
    );
    let map = projected_drawable_map(&trace, &compiled);
    for track in &trace.projected_vertex_tracks {
        let &(layer, index) = map.get(&track.actor).expect("local image sprite");
        let SongLuaOverlayKind::Sprite { texture_key, .. } = &compiled[layer].overlays[index].kind
        else {
            panic!("local image must be a sprite");
        };
        warm_lua_image(
            texture_key,
            (track.texture_size[0] as u32, track.texture_size[1] as u32),
        );
    }
    let draws = compare_image_draws(&trace, &compiled, &context);
    eprintln!("I (Ai) actual image draw checks: {draws}");
    assert_eq!(draws, 598, "include both completed zero-size TV effects");
    let &(layer, index) = map.get(&tv.actor).expect("TV sprite");
    let mut wrong = compiled.clone();
    let fade = wrong[layer]
        .overlay_updates
        .iter_mut()
        .find(|track| {
            track.overlay_index == index && track.target == SongLuaOverlayUpdateTarget::Diffuse
        })
        .expect("TV alpha track");
    for sample in &mut fade.samples {
        if let SongLuaOverlayUpdateValue::Vec4(value) = &mut sample.value {
            value[3] = 0.95;
        }
    }
    let mut rejected = Parity::default();
    compare_drawable_frames(&trace, &wrong, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "incorrect TV alpha must fail full-frame checks"
    );
}

#[test]
fn oshama_whole_native() {
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/oshama-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    assert_eq!(compiled.len(), 2);
    assert_eq!(trace.projected_vertex_tracks.len(), 8);
    assert_eq!(trace.fixture_context.note_end_beat, Some(608.0));
    assert_eq!(trace.end_position.beat, Some(624.0));
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    compare_drawable_frames(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Oshama Scramble whole song"));
    parity.assert_complete("Oshama Scramble whole song");
    assert_eq!(trace.update_frames.len(), 8338);
    assert_eq!(
        parity.checks(),
        304077,
        "retain every observation and quiet frame"
    );
    let map = projected_drawable_map(&trace, &compiled);
    for track in &trace.projected_vertex_tracks {
        let &(layer, index) = map.get(&track.actor).expect("local image sprite");
        let SongLuaOverlayKind::Sprite { texture_key, .. } = &compiled[layer].overlays[index].kind
        else {
            panic!("local image must be a sprite");
        };
        warm_lua_image(
            texture_key,
            (track.texture_size[0] as u32, track.texture_size[1] as u32),
        );
    }
    let draws = compare_image_draws(&trace, &compiled, &context);
    eprintln!("Oshama Scramble actual image draw checks: {draws}");
    assert_eq!(draws, 2440, "retain image geometry, tint and culled output");
    let ending = trace
        .projected_vertex_tracks
        .iter()
        .find(|track| track.texture.ends_with("MilkAngry.png"))
        .expect("final ending sprite");
    assert!(ending.samples.iter().any(
        |sample| sample[0].as_f64().is_some_and(|beat| beat > 618.0)
            && sample[2] == true
            && sample[3] == 1
    ));
    let &(layer, index) = map.get(&ending.actor).expect("ending sprite");
    let mut wrong = compiled.clone();
    let fade = wrong[layer]
        .overlay_updates
        .iter_mut()
        .find(|track| {
            track.overlay_index == index && track.target == SongLuaOverlayUpdateTarget::Diffuse
        })
        .expect("ending alpha track");
    for sample in &mut fade.samples {
        if let SongLuaOverlayUpdateValue::Vec4(value) = &mut sample.value {
            value[3] = 0.95;
        }
    }
    let mut rejected = Parity::default();
    compare_drawable_frames(&trace, &wrong, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "incorrect ending alpha must fail full-frame checks"
    );
}

#[test]
fn sharkmode_whole_song_matches_native() {
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/sharkmode-whole-song.json.zst"),
    );
    assert_eq!(trace.oracle, "itgmania_song_lua_headless_semantic_trace");
    assert_eq!(trace.steps_type, "dance-single");
    assert_eq!(trace.difficulty, "Difficulty_Challenge");
    assert!(trace.end_position.beat.is_some_and(|beat| beat >= 400.0));
    assert_eq!(trace.manual_draw_frames.len(), 9292);
    assert_eq!(
        trace
            .manual_draw_frames
            .iter()
            .flat_map(|(_, _, ops)| ops)
            .filter(|op| { op["path"] == "ScreenGameplay/In" && op["operation"] == "draw" })
            .count(),
        1351,
        "retain explicit stage-label draws"
    );
    let song = parse_song(&locate_simfile(&trace));
    assert!(
        song.charts.iter().any(|chart| {
            chart.chart_type == "dance-single"
                && chart.difficulty == "Challenge"
                && chart.short_hash == "f13226ac1b5eedcc"
        }),
        "retain the frozen Sharkmode chart identity"
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary("Sharkmode whole song"));
    parity.assert_complete("Sharkmode whole song");
    assert_eq!(parity.checks(), 109825, "retain every existing comparison");
}

#[test]
fn karachi_whole_native() {
    use deadsync_assets::song_lua::SongLuaProxyTarget as Target;
    use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
    crate::paths::init();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua-micro/karachi-whole-song.json.zst"),
    );
    let (compiled, primary, context) = compile_trace_song(&trace);
    assert_eq!(
        trace.projected_vertex_tracks.len(),
        8,
        "retain six named capture sprites"
    );
    assert_eq!(trace.update_frames.len(), 7576, "retain the entire song");
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    compare_drawable_frames(&trace, &compiled, &context, &mut parity);
    let map = projected_drawable_map(&trace, &compiled);
    parity.section("capture proxy bindings");
    let mut targets = 0;
    for track in &trace.tween_tracks {
        for operation in track
            .segments
            .iter()
            .flat_map(|segment| &segment.operations)
            .filter(|operation| operation.operation == "ActorProxy.SetTarget")
        {
            let path = operation.args[0]["path"]
                .as_str()
                .expect("native proxy target path");
            let expected = match path {
                "ScreenGameplay/Underlay" => Target::Underlay { hidden: false },
                "ScreenGameplay/Overlay" => Target::Overlay { hidden: false },
                "ScreenGameplay/PlayerP1/Combo" => Target::Combo { player_index: 0 },
                "ScreenGameplay/PlayerP2/Combo" => Target::Combo { player_index: 1 },
                "ScreenGameplay/PlayerP1/NoteField" => Target::NoteField { player_index: 0 },
                "ScreenGameplay/PlayerP2/NoteField" => Target::NoteField { player_index: 1 },
                "ScreenGameplay/PlayerP1/Judgment" => Target::Judgment { player_index: 0 },
                "ScreenGameplay/PlayerP2/Judgment" => Target::Judgment { player_index: 1 },
                _ => panic!("unexpected retained proxy target {path}"),
            };
            let &(layer, index) = map.get(&track.actor).expect("capture proxy actor");
            parity.check(
                matches!(compiled[layer].overlays[index].kind,
                SongLuaOverlayKind::ActorProxy { target } if target == expected),
                || {
                    format!(
                        "{} must capture {path}: {:?}",
                        track.actor, compiled[layer].overlays[index].kind
                    )
                },
            );
            targets += 1;
        }
    }
    assert_eq!(
        targets, 8,
        "include both players' note fields, judgments, combo and HUD"
    );
    parity.section("named capture resources");
    let captures = trace
        .projected_vertex_tracks
        .iter()
        .filter(|track| track.texture.starts_with("aft:"))
        .collect::<Vec<_>>();
    assert_eq!(captures.len(), 6);
    for track in &captures {
        let &(layer, index) = map.get(&track.actor).expect("capture sprite");
        let &(owner_layer, owner) = map
            .get(track.texture.strip_prefix("aft:").unwrap())
            .expect("native capture owner");
        let state = compiled_overlay_states_at(&compiled[owner_layer], &context, 0.0, 0.0)[owner];
        parity.check(state.size == Some(track.texture_size), || {
            format!("{} capture dimensions", track.actor)
        });
        parity.check(matches!((&compiled[layer].overlays[index].kind,
            &compiled[owner_layer].overlays[owner].kind),
            (SongLuaOverlayKind::AftSprite { capture_name: sprite },
             SongLuaOverlayKind::ActorFrameTexture { capture_name: capture, .. }) if sprite == capture),
            || format!("{} must sample {}",track.actor,track.texture));
    }
    eprintln!("{}", parity.summary("Karachi whole song"));
    parity.assert_complete("Karachi whole song");
    assert_eq!(
        parity.checks(),
        200040,
        "retain every observation, binding and quiet drawable frame"
    );
    let background = trace
        .projected_vertex_tracks
        .iter()
        .find(|track| track.texture.ends_with(".png"))
        .expect("local PNG background");
    let &(layer, index) = map.get(&background.actor).expect("PNG sprite");
    let SongLuaOverlayKind::Sprite { texture_key, .. } = &compiled[layer].overlays[index].kind
    else {
        panic!("PNG background must be a sprite")
    };
    warm_lua_image(texture_key, (1280, 720));
    let draws = compare_image_draws(&trace, &compiled, &context);
    eprintln!("Karachi actual image/capture draw checks: {draws}");
    assert_eq!(
        draws, 7600,
        "retain every visible PNG and render-target sample"
    );
    let mut composer = WholeSongComposer::new(&compiled[primary].overlays);
    let mut capture_handle = None;
    for track in &captures {
        let &(_, index) = map.get(&track.actor).unwrap();
        let sample = track
            .samples
            .iter()
            .find(|sample| sample[2] == true && sample[3].as_f64().is_some_and(|alpha| alpha > 0.0))
            .expect("visible native capture sample");
        let beat = value_f32(sample.get(0)).unwrap();
        let seconds = value_f32(sample.get(1)).unwrap();
        let states = compiled_overlay_states_at(&compiled[primary], &context, beat, seconds);
        let frame = composer.render_overlay(
            &compiled[primary].overlays,
            &states,
            index,
            [854.0, 480.0],
            seconds,
            beat,
        );
        let deadlib_present::render::DrawOp::Sprite(run) = frame.ops[0] else {
            panic!("capture sprite draw")
        };
        let handle = deadlib_present::render::render_target_base_handle(run.texture_handle);
        assert_eq!(
            *capture_handle.get_or_insert(handle),
            handle,
            "all copies must sample the same target"
        );
        let expected =
            if ["def-0016", "def-0017", "def-0018", "def-0019"].contains(&track.actor.as_str()) {
                deadlib_present::render::BlendMode::Add
            } else {
                deadlib_present::render::BlendMode::Alpha
            };
        assert_eq!(run.blend, expected, "native glow/scringus blend");
    }
    let mut wrong = compiled.clone();
    let &(layer, index) = map.get(&captures[0].actor).unwrap();
    let fade = wrong[layer]
        .overlay_updates
        .iter_mut()
        .find(|track| {
            track.overlay_index == index && track.target == SongLuaOverlayUpdateTarget::Diffuse
        })
        .expect("native glow alpha track");
    for sample in &mut fade.samples {
        if let SongLuaOverlayUpdateValue::Vec4(value) = &mut sample.value {
            value[3] = 0.95;
        }
    }
    let mut rejected = Parity::default();
    compare_drawable_frames(&trace, &wrong, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "incorrect capture glow must fail full-frame checks"
    );
}
