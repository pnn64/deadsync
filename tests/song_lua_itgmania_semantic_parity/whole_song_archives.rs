use super::*;
use deadsync_theme_simply_love::screens::gameplay::actor_conformance::{
    WholeSongComposer, compose_overlay_states,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read;

const ARCHIVE_SCHEMA_VERSION: u32 = 1;
const ARCHIVE_FILTER_ENV: &str = "DEADSYNC_SONG_ARCHIVE";

#[derive(Deserialize)]
struct ArchiveIndex {
    archive_schema_version: u32,
    hash: String,
    archives: Vec<ArchiveEntry>,
}

#[derive(Deserialize)]
struct ArchiveEntry {
    title: String,
    source_simfile: String,
    archive: String,
    sha256: String,
    compressed_bytes: u64,
}

#[derive(Deserialize)]
struct ArchiveManifest {
    archive_schema_version: u32,
    fixture_schema_version: u32,
    oracle_schema_version: u32,
    itgmania: ArchiveItgmania,
    chart: ArchiveChart,
    runtime: ArchiveRuntime,
    lua_closure: LuaClosure,
    textures: Vec<TextureMetadata>,
    required_assets: Vec<AssetReference>,
    files: Vec<ArchiveFile>,
}

#[derive(Deserialize)]
struct ArchiveItgmania {
    source_revision: String,
    execution: String,
    launches_executable: bool,
}

#[derive(Deserialize)]
struct ArchiveChart {
    title: String,
    source_path: String,
    simfile: String,
    trace: String,
}

#[derive(Deserialize)]
struct ArchiveRuntime {
    display: ArchiveDisplay,
    update_hz: f32,
    random_state: RandomState,
}

#[derive(Deserialize)]
struct ArchiveDisplay {
    width: f32,
    height: f32,
    logical_width: f32,
    logical_height: f32,
}

#[derive(Deserialize)]
struct RandomState {
    source: String,
    seed: Option<u64>,
    reproducible: bool,
}

#[derive(Deserialize)]
struct LuaClosure {
    strategy: String,
    files: Vec<String>,
    external_references: Vec<String>,
}

#[derive(Deserialize)]
struct TextureMetadata {
    reference: String,
    width: Option<u32>,
    height: Option<u32>,
    local_path: Option<String>,
}

#[derive(Deserialize)]
struct AssetReference {
    reference: String,
    kind: String,
    local_path: Option<String>,
    exists: bool,
}

#[derive(Deserialize)]
struct ArchiveFile {
    path: String,
    role: String,
    bytes: usize,
    sha256: String,
}

struct ExtractedArchive {
    _temp: tempfile::TempDir,
    root: PathBuf,
    manifest: ArchiveManifest,
}

fn archive_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/itgmania-song-archives")
}

fn archive_index() -> ArchiveIndex {
    let path = archive_root().join("index.json");
    serde_json::from_slice(
        &fs::read(&path).unwrap_or_else(|error| {
            panic!("missing song archive index {}: {error}", path.display())
        }),
    )
    .unwrap_or_else(|error| panic!("invalid song archive index {}: {error}", path.display()))
}

fn selected_archives(index: &ArchiveIndex) -> Vec<&ArchiveEntry> {
    let filter = std::env::var(ARCHIVE_FILTER_ENV).ok();
    let selected = index
        .archives
        .iter()
        .filter(|entry| {
            filter.as_ref().is_none_or(|filter| {
                entry.title.contains(filter)
                    || entry.source_simfile.contains(filter)
                    || entry.sha256.starts_with(filter)
            })
        })
        .collect::<Vec<_>>();
    assert!(
        !selected.is_empty(),
        "{ARCHIVE_FILTER_ENV} did not match a whole-song archive"
    );
    selected
}

fn extract_archive(entry: &ArchiveEntry) -> ExtractedArchive {
    let path = archive_root().join(&entry.archive);
    let metadata = fs::metadata(&path)
        .unwrap_or_else(|error| panic!("missing archive {}: {error}", path.display()));
    assert_eq!(metadata.len(), entry.compressed_bytes, "archive byte count");
    assert_eq!(hash_file(&path), entry.sha256, "archive content address");
    assert_eq!(entry.archive, format!("{}.tar.zst", entry.sha256));

    let temp = tempfile::tempdir().expect("create archive extraction directory");
    let input = File::open(&path).expect("open whole-song archive");
    let decoder = zstd::stream::read::Decoder::new(input).expect("start streaming zstd decoder");
    let mut archive = tar::Archive::new(decoder);
    for member in archive.entries().expect("read tar entries") {
        let mut member = member.expect("read tar member");
        assert!(
            member
                .unpack_in(temp.path())
                .expect("stream archive member"),
            "archive member escaped extraction root"
        );
    }
    let manifest_path = temp.path().join("manifest.json");
    let manifest =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read extracted archive manifest"))
            .expect("parse extracted archive manifest");
    ExtractedArchive {
        root: temp.path().to_owned(),
        _temp: temp,
        manifest,
    }
}

fn validate_archive(entry: &ArchiveEntry, archive: &ExtractedArchive) {
    let manifest = &archive.manifest;
    assert_eq!(manifest.archive_schema_version, ARCHIVE_SCHEMA_VERSION);
    assert!(manifest.fixture_schema_version > 0);
    assert!(manifest.oracle_schema_version > 0);
    assert_eq!(manifest.chart.title, entry.title);
    assert_eq!(manifest.chart.source_path, entry.source_simfile);
    assert!(!manifest.itgmania.source_revision.is_empty());
    assert_eq!(manifest.itgmania.execution, "embedded_bundled_lua");
    assert!(!manifest.itgmania.launches_executable);
    assert!(manifest.runtime.update_hz.is_finite() && manifest.runtime.update_hz > 0.0);
    assert!(manifest.runtime.display.width > 0.0);
    assert!(manifest.runtime.display.height > 0.0);
    assert!(manifest.runtime.display.logical_width > 0.0);
    assert!(manifest.runtime.display.logical_height > 0.0);
    assert!(!manifest.runtime.random_state.source.is_empty());
    assert_eq!(
        manifest.runtime.random_state.reproducible,
        manifest.runtime.random_state.seed.is_some()
    );
    assert_eq!(
        manifest.lua_closure.strategy,
        "executed-sources-plus-static-loads"
    );
    assert!(!manifest.lua_closure.files.is_empty());
    for lua in &manifest.lua_closure.files {
        assert!(lua.ends_with(".lua"), "non-Lua closure member: {lua}");
        assert!(
            archive.root.join(lua).is_file(),
            "missing Lua member: {lua}"
        );
    }
    for reference in &manifest.lua_closure.external_references {
        assert!(!reference.is_empty());
    }
    for texture in &manifest.textures {
        assert!(!texture.reference.is_empty());
        assert_eq!(texture.width.is_some(), texture.height.is_some());
        if let Some(local_path) = &texture.local_path {
            assert!(
                manifest
                    .required_assets
                    .iter()
                    .any(|asset| asset.local_path.as_ref() == Some(local_path) && asset.exists),
                "resolved texture is not in required asset references: {local_path}"
            );
        }
    }
    for asset in &manifest.required_assets {
        assert!(!asset.reference.is_empty());
        assert!(matches!(
            asset.kind.as_str(),
            "texture" | "audio" | "shader" | "other"
        ));
    }
    for file in &manifest.files {
        let path = archive.root.join(&file.path);
        let bytes = fs::read(&path)
            .unwrap_or_else(|error| panic!("missing archive member {}: {error}", path.display()));
        assert_eq!(bytes.len(), file.bytes, "member size: {}", file.path);
        assert_eq!(
            hash_bytes(&bytes),
            file.sha256,
            "member hash: {}",
            file.path
        );
        assert!(matches!(
            file.role.as_str(),
            "simfile" | "semantic-render-trace" | "lua-dependency" | "required-asset"
        ));
    }
    assert!(archive.root.join(&manifest.chart.simfile).is_file());
    assert!(archive.root.join(&manifest.chart.trace).is_file());
}

fn compile_archive(
    archive: &ExtractedArchive,
) -> (
    NativeTrace,
    Vec<CompiledSongLua>,
    usize,
    SongLuaCompileContext,
) {
    let trace_path = archive.root.join(&archive.manifest.chart.trace);
    let trace = read_trace_file(&trace_path);
    let simfile = archive.root.join(&archive.manifest.chart.simfile);
    let (compiled, primary_index, context) = compile_trace_song_at(&trace, &simfile);
    (trace, compiled, primary_index, context)
}

fn compose_entire_song(
    trace: &NativeTrace,
    compiled_layers: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    update_hz: f32,
) {
    let frame_count = (trace.end_position.seconds.max(0.0) * update_hz).ceil() as usize + 1;
    let composers = compiled_layers
        .iter()
        .map(|compiled| WholeSongComposer::new(&compiled.overlays))
        .collect::<Vec<_>>();
    let mut actor_samples = 0usize;
    for frame in 0..frame_count {
        let seconds = frame as f32 / update_hz;
        let beat = song_beat_at_elapsed_seconds(seconds, context);
        for (compiled, composer) in compiled_layers.iter().zip(&composers) {
            let local = compiled
                .overlays
                .iter()
                .enumerate()
                .map(|(overlay_index, _)| {
                    let mut state =
                        compiled_message_state_at(context, compiled, overlay_index, beat, seconds);
                    apply_runtime_updates(compiled, overlay_index, beat, &mut state);
                    state
                })
                .collect::<Vec<_>>();
            let composed = compose_overlay_states(
                &compiled.overlays,
                &local,
                [compiled.screen_width, compiled.screen_height],
            );
            assert_eq!(composed.len(), compiled.overlays.len());
            actor_samples += composer.actor_count(
                &compiled.overlays,
                &composed,
                [compiled.screen_width, compiled.screen_height],
                seconds,
                beat,
            );
            for (index, state) in composed.iter().enumerate() {
                let values = [
                    state.x,
                    state.y,
                    state.z,
                    state.zoom,
                    state.zoom_x,
                    state.zoom_y,
                    state.zoom_z,
                    state.rot_x_deg,
                    state.rot_y_deg,
                    state.rot_z_deg,
                    state.skew_x,
                    state.skew_y,
                    state.diffuse[0],
                    state.diffuse[1],
                    state.diffuse[2],
                    state.diffuse[3],
                ];
                assert!(
                    values.iter().all(|value| value.is_finite()),
                    "non-finite composed state at frame {frame}, actor {index}"
                );
            }
        }
    }
    // Modifier-only songs can have no drawable overlays. Require draw output
    // only when the reference actually sampled a visible primitive.
    let native_draws = trace.projected_vertex_tracks.iter().any(|track| {
        track.samples.iter().any(|sample| {
            sample.get(2).and_then(Value::as_bool) == Some(true)
                && value_f32(sample.get(3)).is_some_and(|alpha| alpha > 0.000_001)
        })
    });
    assert!(
        !native_draws || actor_samples > 0,
        "whole-song composition emitted no actors despite visible reference geometry"
    );
}

fn assert_complete_parity(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    primary_index: usize,
    context: &SongLuaCompileContext,
) {
    let mut gaps = Vec::new();
    compare_compile_info(compiled, &mut gaps);
    compare_layers(trace, compiled, &mut gaps);
    compare_final_render_states(trace, compiled, &mut gaps);
    compare_update_render_persistence(trace, compiled, &mut gaps);
    compare_update_render_values(trace, compiled, context, &mut gaps);
    compare_player_operation_ranges(trace, compiled, &mut gaps);
    compare_column_splines(trace, compiled, context, &mut gaps);
    compare_projected_geometry(trace, compiled, context, &mut gaps);
    compare_projected_vibration_coverage(trace, compiled, context, &mut gaps);
    compare_timeline(trace, &compiled[primary_index], &mut gaps);
    compare_commands(trace, compiled, primary_index, &mut gaps);
    assert!(
        gaps.is_empty(),
        "whole-song archive parity gaps ({}):\n- {}",
        gaps.len(),
        gaps.join("\n- ")
    );
}

fn hash_file(path: &Path) -> String {
    let mut file = File::open(path).expect("open archive for hashing");
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).expect("hash archive");
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    encode_hash(hasher.finalize())
}

fn hash_bytes(bytes: &[u8]) -> String {
    encode_hash(Sha256::digest(bytes))
}

fn encode_hash(hash: impl AsRef<[u8]>) -> String {
    let bytes = hash.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[test]
fn whole_song_archive_index_and_streamed_members_are_valid() {
    let index = archive_index();
    assert_eq!(index.archive_schema_version, ARCHIVE_SCHEMA_VERSION);
    assert_eq!(index.hash, "sha256-compressed-archive");
    assert_eq!(index.archives.len(), 45);
    for entry in selected_archives(&index) {
        let archive = extract_archive(entry);
        validate_archive(entry, &archive);
    }
}

#[test]
#[ignore = "explicit full-corpus compile, composition, and exact semantic/render audit"]
fn whole_song_archives_compile_compose_and_match_native_trace() {
    let index = archive_index();
    for entry in selected_archives(&index) {
        eprintln!("whole-song parity: {}", entry.source_simfile);
        let archive = extract_archive(entry);
        validate_archive(entry, &archive);
        let (trace, compiled, primary_index, context) = compile_archive(&archive);
        compose_entire_song(
            &trace,
            &compiled,
            &context,
            archive.manifest.runtime.update_hz,
        );
        assert_complete_parity(&trace, &compiled, primary_index, &context);
    }
}
