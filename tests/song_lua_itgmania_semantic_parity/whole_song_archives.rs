use super::*;
use deadsync_song_lua::playback::actor_conformance::{WholeSongComposer, compose_overlay_states};
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
    harness_version: String,
    archive: String,
    sha256: String,
    compressed_bytes: u64,
    #[serde(default)]
    local_only: bool,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(skip)]
    retained_version: bool,
}

#[derive(Deserialize)]
struct ArchiveManifest {
    archive_schema_version: u32,
    fixture_schema_version: u32,
    oracle_schema_version: u32,
    harness_version: String,
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/full_song_lua")
}

/// Resolve historical regression references into the single flat fixture store.
/// Aliases retain the original native captures, including older oracle versions.
pub(super) fn reference_path(path: &Path) -> PathBuf {
    if path.is_file() {
        return path.to_owned();
    }
    static REFERENCES: std::sync::OnceLock<HashMap<String, String>> = std::sync::OnceLock::new();
    let references = REFERENCES.get_or_init(|| {
        serde_json::from_slice(
            &fs::read(archive_root().join("references.json"))
                .expect("missing consolidated song Lua reference index"),
        )
        .expect("invalid consolidated song Lua reference index")
    });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Normalize '..' lexically: historical manifests can reference a micro trace
    // outside their former directory, and those directories no longer exist.
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            _ => return path.to_owned(),
        }
    }
    references
        .get(&parts.join("/"))
        .map_or_else(|| path.to_owned(), |name| archive_root().join(name))
}

pub(super) fn reference_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let path = reference_path(path);
    let input = File::open(&path)
        .unwrap_or_else(|error| panic!("missing reference {}: {error}", path.display()));
    if path.extension().is_some_and(|extension| extension == "zst") {
        let decoder = zstd::stream::read::Decoder::new(input).expect("decode reference");
        serde_json::from_reader(std::io::BufReader::new(decoder)).expect("parse reference JSON")
    } else {
        serde_json::from_reader(std::io::BufReader::new(input)).expect("parse reference JSON")
    }
}

fn archive_index() -> ArchiveIndex {
    let path = archive_root().join("index.json");
    let mut index: ArchiveIndex =
        serde_json::from_slice(&fs::read(&path).unwrap_or_else(|error| {
            panic!("missing song archive index {}: {error}", path.display())
        }))
        .unwrap_or_else(|error| panic!("invalid song archive index {}: {error}", path.display()));
    // Retained native versions are runnable by filename as well as the current
    // captures in the index. Discover them in the same flat fixture store.
    let indexed = index
        .archives
        .iter()
        .map(|entry| entry.archive.clone())
        .collect::<HashSet<_>>();
    let aliases: HashMap<String, String> = fs::read(archive_root().join("references.json"))
        .map(|bytes| serde_json::from_slice(&bytes).expect("reference aliases"))
        .unwrap_or_default();
    let mut extra = Vec::new();
    for file in fs::read_dir(archive_root()).expect("list song archives") {
        let file = file.expect("archive directory entry");
        let name = file.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".tar.zst") || indexed.contains(&name) {
            continue;
        }
        let decoder = zstd::stream::read::Decoder::new(
            File::open(file.path()).expect("open retained archive"),
        )
        .expect("decode retained archive");
        let mut archive = tar::Archive::new(decoder);
        let mut members = archive.entries().expect("retained archive members");
        let member = members
            .next()
            .expect("retained archive manifest")
            .expect("read manifest member");
        assert_eq!(member.path().unwrap(), Path::new("manifest.json"));
        let manifest: ArchiveManifest =
            serde_json::from_reader(member).expect("retained archive manifest JSON");
        let mut historical = aliases
            .iter()
            .filter(|(_, target)| **target == name)
            .filter_map(|(alias, _)| {
                Path::new(alias)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect::<Vec<_>>();
        historical.sort();
        extra.push(ArchiveEntry {
            title: manifest.chart.title,
            source_simfile: manifest.chart.source_path,
            harness_version: manifest.harness_version,
            sha256: name.trim_end_matches(".tar.zst").into(),
            compressed_bytes: file.metadata().unwrap().len(),
            archive: name,
            aliases: historical,
            local_only: false,
            retained_version: true,
        });
    }
    extra.sort_by(|a, b| a.archive.cmp(&b.archive));
    index.archives.extend(extra);
    let unavailable = index
        .archives
        .iter()
        .filter(|entry| !archive_available(entry))
        .count();
    if unavailable > 0 {
        eprintln!(
            "{unavailable} local-only archives are unavailable; generate them with the harness to run their comparisons"
        );
    }
    index
}

fn archive_available(entry: &ArchiveEntry) -> bool {
    !entry.local_only || archive_root().join(&entry.archive).is_file()
}

fn selected_archives(index: &ArchiveIndex) -> Vec<&ArchiveEntry> {
    let filter = std::env::var(ARCHIVE_FILTER_ENV).ok();
    let selected = index
        .archives
        .iter()
        .filter(|entry| archive_available(entry))
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
    assert_eq!(
        entry.harness_version, archive.manifest.harness_version,
        "fixture index must preserve the capture's harness version"
    );
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
    // A recorded seed alone does not guarantee deterministic native state.
    // Preserve captures explicitly marked as non-reproducible.
    assert!(
        !manifest.runtime.random_state.reproducible || manifest.runtime.random_state.seed.is_some(),
        "a reproducible capture must record its seed"
    );
    assert_eq!(
        manifest.lua_closure.strategy,
        "executed-sources-plus-static-loads"
    );
    if manifest.lua_closure.files.is_empty() {
        let song = parse_song(&archive.root.join(&manifest.chart.simfile));
        assert!(
            song.background_lua_changes.is_empty() && song.foreground_lua_changes.is_empty(),
            "empty Lua closure omitted authored song layers"
        );
    }
    for lua in &manifest.lua_closure.files {
        assert!(lua.ends_with(".lua"), "non-Lua closure member: {lua}");
        assert!(
            archive.root.join(lua).is_file(),
            "missing Lua member: {lua}"
        );
        assert!(
            manifest.files.iter().any(|file| file.path == *lua),
            "Lua closure member is not hash-validated: {lua}"
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
        assert!(
            Path::new(&file.path)
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
            "invalid archive member path"
        );
        let path = archive.root.join(&file.path);
        assert_eq!(
            fs::metadata(&path).expect("archive member exists").len(),
            file.bytes as u64,
            "member size: {}",
            file.path
        );
        assert_eq!(hash_file(&path), file.sha256, "member hash: {}", file.path);
        assert!(matches!(
            file.role.as_str(),
            "simfile" | "semantic-render-trace" | "lua-dependency" | "required-asset"
        ));
    }
    assert!(archive.root.join(&manifest.chart.simfile).is_file());
    assert!(archive.root.join(&manifest.chart.trace).is_file());
}

// Full-song archives must describe native engine timing and complete replay.
// Focused Lua-only micro fixtures can legitimately use a synthetic clock.
fn validate_native_trace(trace: &NativeTrace, manifest: &ArchiveManifest) {
    assert_eq!(
        trace.harness_version, manifest.harness_version,
        "trace and manifest capture versions differ"
    );
    assert!(
        matches!(
            trace.song_clock.as_deref(),
            Some("native-song-timing" | "native-pauses")
        ),
        "obsolete song clock {:?}; recapture this archive with native timing",
        trace.song_clock,
    );
    assert!(
        trace.runtime_errors.is_empty(),
        "native runtime errors invalidate the reference"
    );
    assert_eq!(
        trace.dropped_events, 0,
        "dropped native events invalidate the reference"
    );
    assert!(
        trace
            .update_frames
            .first()
            .is_some_and(|frame| frame.1 == 0.0)
            && trace.update_frames.last().is_some_and(|frame| {
                frame.1 as f32 >= trace.end_position.seconds
                    && trace
                        .trace_until_seconds
                        .is_none_or(|end| frame.1 + 0.0000001 >= end)
            }),
        "native replay must cover the complete song from frame zero",
    );
    let hibernates = |operation: &str, args: &[Value]| {
        operation
            .rsplit('.')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("hibernate"))
            && args
                .first()
                .and_then(projected_alpha)
                .is_some_and(|value| value > 0.0)
    };
    let positive_hibernate = trace.operation_tracks.iter().any(|track| {
        track
            .samples
            .iter()
            .any(|sample| hibernates(&track.operation, &sample.3))
    }) || trace
        .tween_tracks
        .iter()
        .flat_map(|track| &track.segments)
        .flat_map(|segment| &segment.operations)
        .any(|operation| hibernates(&operation.operation, &operation.args));
    let version = manifest
        .harness_version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .expect("numeric harness version");
    assert!(
        !positive_hibernate || version.as_slice() >= [0, 1, 6].as_slice(),
        "obsolete hibernation replay; recapture with harness 0.1.6 or later",
    );
}

fn compose_entire_song_with_progress(
    trace: &NativeTrace,
    compiled_layers: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    update_hz: f32,
    progress: Option<&progress::Progress>,
) {
    use std::sync::atomic::Ordering;
    let frame_count = (trace.end_position.seconds.max(0.0) * update_hz).ceil() as usize + 1;
    if let Some(progress) = progress {
        progress.frames.store(frame_count, Ordering::Relaxed);
        progress.frame.store(0, Ordering::Relaxed);
    }
    let composers = compiled_layers
        .iter()
        .map(|compiled| WholeSongComposer::new(&compiled.overlays))
        .collect::<Vec<_>>();
    let mut actor_samples = 0usize;
    for frame in 0..frame_count {
        let seconds = frame as f32 / update_hz;
        let beat = song_beat_at_elapsed_seconds(seconds, context);
        for (compiled, composer) in compiled_layers.iter().zip(&composers) {
            let local = compiled_local_states_at(compiled, context, beat, seconds);
            let composed = compose_overlay_states(
                &compiled.overlays,
                &local,
                [compiled.screen_width, compiled.screen_height],
                [seconds, beat],
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
        if let Some(progress) = progress {
            progress.frame.store(frame + 1, Ordering::Relaxed);
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

pub(super) fn hash_file(path: &Path) -> String {
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
    crate::paths::init();
    let index = archive_index();
    assert_eq!(index.archive_schema_version, ARCHIVE_SCHEMA_VERSION);
    assert_eq!(index.hash, "sha256-compressed-archive");
    assert!(!index.archives.is_empty());
    let mut names = HashSet::new();
    for entry in selected_archives(&index) {
        assert!(names.insert(&entry.archive), "duplicate indexed archive");
        let archive = extract_archive(entry);
        validate_archive(entry, &archive);
        let trace = read_trace_file(&archive.root.join(&archive.manifest.chart.trace));
        validate_native_trace(&trace, &archive.manifest);
        if !entry.retained_version
            && entry
                .source_simfile
                .starts_with("[09] Who the Hell Is Edgar")
        {
            for font in [
                "song/multitap/_komika axis 42px.ini",
                "song/multitap/_komika axis 42px [numbers] 4x4 (doubleres).png",
            ] {
                assert!(
                    archive.manifest.files.iter().any(|file| file.path == font),
                    "the isolated countdown needs {font}"
                );
            }
        }
    }
}

#[test]
fn empty_song_layers_match_native_archive() {
    crate::paths::init();
    let index = archive_index();
    let entry = index
        .archives
        .iter()
        .find(|entry| entry.source_simfile == "Bank Account/Bank Account.sm")
        .expect("complete empty-Lua reference");
    let archive = extract_archive(entry);
    validate_archive(entry, &archive);
    let mut trace = read_trace_file(&archive.root.join(&archive.manifest.chart.trace));
    validate_native_trace(&trace, &archive.manifest);
    let (compiled, primary, context) =
        compile_trace_song_at(&trace, &archive.root.join(&archive.manifest.chart.simfile));
    assert!(compiled.is_empty());
    assert!(trace.roots.is_empty());
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    runtime_modifiers::compare_player_frames(&trace, &compiled, &context, &mut parity);
    parity.assert_complete("song without Lua layers");
    // Missing compiled roots must still fail instead of skipping native actors.
    trace.roots.push("missing-layer".into());
    let rejected = compare_semantics(&trace, &compiled, primary, &context);
    assert!(
        !rejected.gaps.is_empty(),
        "uncompiled native layers must fail"
    );
}

#[test]
fn archive_reference_rejects_obsolete_replays() {
    crate::paths::init();
    let index = archive_index();
    let entry = index
        .archives
        .iter()
        .find(|entry| entry.source_simfile == "Bank Account/Bank Account.sm")
        .expect("complete native reference");
    let mut archive = extract_archive(entry);
    let mut trace = read_trace_file(&archive.root.join(&archive.manifest.chart.trace));
    validate_native_trace(&trace, &archive.manifest);
    trace.song_clock = Some("continuous-bpm".into());
    assert!(std::panic::catch_unwind(|| validate_native_trace(&trace, &archive.manifest)).is_err());
    trace.song_clock = Some("native-song-timing".into());
    trace.operation_tracks.push(NativeOperationTrack {
        actor: "probe".into(),
        operation: "Actor.hibernate".into(),
        samples: vec![(0, 0.0, 0.0, vec![serde_json::json!(0.125)])],
    });
    archive.manifest.harness_version = "0.1.5".into();
    trace.harness_version = "0.1.5".into();
    assert!(std::panic::catch_unwind(|| validate_native_trace(&trace, &archive.manifest)).is_err());
    trace
        .operation_tracks
        .last_mut()
        .expect("hibernation probe")
        .samples[0]
        .3 = vec![serde_json::json!({"type": "number", "value": "infinity"})];
    assert!(std::panic::catch_unwind(|| validate_native_trace(&trace, &archive.manifest)).is_err());
    archive.manifest.harness_version = "0.1.6".into();
    trace.harness_version = "0.1.6".into();
    validate_native_trace(&trace, &archive.manifest);
    trace.dropped_events = 1;
    assert!(std::panic::catch_unwind(|| validate_native_trace(&trace, &archive.manifest)).is_err());
    trace.dropped_events = 0;
    trace.runtime_errors.push(serde_json::json!("native error"));
    assert!(std::panic::catch_unwind(|| validate_native_trace(&trace, &archive.manifest)).is_err());
    trace.runtime_errors.clear();
    trace.update_frames.clear();
    assert!(std::panic::catch_unwind(|| validate_native_trace(&trace, &archive.manifest)).is_err());
}

const USAGE: &str = "Run one complete fixture:\n  cargo test --test full_song_lua <archive.tar.zst>\n\nList fixtures:\n  cargo test --test full_song_lua -- --list\n\nRun the complete corpus:\n  cargo test --test full_song_lua -- --all\n\nThe positional selector accepts a filename, SHA-256 prefix, or song title.\nNo selector prints this help; normal cargo test does not run the expensive corpus.";

fn select_archive<'a>(
    index: &'a ArchiveIndex,
    selector: &str,
) -> Result<&'a ArchiveEntry, Vec<&'a ArchiveEntry>> {
    let filename = Path::new(selector)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(selector);
    let exact = index
        .archives
        .iter()
        .filter(|entry| {
            entry.archive == filename
                || entry.aliases.iter().any(|alias| alias == filename)
                || entry.title.eq_ignore_ascii_case(selector)
                || entry.source_simfile.eq_ignore_ascii_case(selector)
        })
        .collect::<Vec<_>>();
    let matches = if exact.is_empty() {
        index
            .archives
            .iter()
            .filter(|entry| {
                entry.sha256.starts_with(selector)
                    || entry
                        .aliases
                        .iter()
                        .any(|alias| alias.starts_with(selector))
                    || entry
                        .title
                        .to_lowercase()
                        .contains(&selector.to_lowercase())
                    || entry
                        .source_simfile
                        .to_lowercase()
                        .contains(&selector.to_lowercase())
            })
            .collect::<Vec<_>>()
    } else {
        exact
    };
    if matches.len() == 1 {
        Ok(matches[0])
    } else {
        Err(matches)
    }
}

pub(crate) fn run_cli(mut args: Vec<String>) -> std::process::ExitCode {
    use std::process::ExitCode;
    // Cargo forwards these global libtest options to custom harnesses too.
    // This harness always shows output and explicitly selects expensive runs.
    args.retain(|arg| {
        !matches!(
            arg.as_str(),
            "--nocapture"
                | "--show-output"
                | "--ignored"
                | "--include-ignored"
                | "--exact"
                | "--quiet"
        )
    });
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let index = archive_index();
    if args == ["--list"] {
        for entry in &index.archives {
            println!(
                "{}  {}  ({})",
                entry.archive, entry.title, entry.source_simfile
            );
        }
        println!("{} full-song archives", index.archives.len());
        return ExitCode::SUCCESS;
    }
    let selected = if args == ["--all"] {
        index
            .archives
            .iter()
            .filter(|entry| archive_available(entry))
            .collect::<Vec<_>>()
    } else if args.len() == 1 && !args[0].starts_with('-') {
        match select_archive(&index, &args[0]) {
            Ok(entry) if archive_available(entry) => vec![entry],
            Ok(entry) => {
                eprintln!(
                    "{} is a local-only archive; generate it with the harness before selecting it",
                    entry.archive
                );
                return ExitCode::FAILURE;
            }
            Err(matches) => {
                eprintln!(
                    "selector {:?} matched {} archives; choose one filename from --list",
                    args[0],
                    matches.len()
                );
                for entry in matches.iter().take(20) {
                    eprintln!("  {}  {}", entry.archive, entry.source_simfile);
                }
                return ExitCode::FAILURE;
            }
        }
    } else {
        eprintln!("invalid arguments: {}\n\n{USAGE}", args.join(" "));
        return ExitCode::FAILURE;
    };
    crate::paths::init();
    let started = std::time::Instant::now();
    let mut failed = 0;
    let mut checks = 0;
    let mut failed_checks = 0;
    for (position, entry) in selected.iter().enumerate() {
        eprintln!(
            "\n[{}/{}] {} ({})\n{}\n  harness version: {}",
            position + 1,
            selected.len(),
            entry.title,
            entry.source_simfile,
            entry.archive,
            entry.harness_version
        );
        let reporter = progress::Reporter::start();
        let progress = &reporter.progress;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            progress.stage("extracting archive and verifying SHA-256");
            let archive = extract_archive(entry);
            progress.stage("validating archive members");
            validate_archive(entry, &archive);
            progress.stage("reading native trace");
            let trace = read_trace_file(&archive.root.join(&archive.manifest.chart.trace));
            validate_native_trace(&trace, &archive.manifest);
            eprintln!(
                "  native duration: {:.2}s, {} recorded update frames",
                trace.end_position.seconds,
                trace.update_frames.len()
            );
            progress.stage("compiling complete Lua runtime");
            let (compiled, primary, context) =
                compile_trace_song_at(&trace, &archive.root.join(&archive.manifest.chart.simfile));
            progress.stage("composing every song frame");
            compose_entire_song_with_progress(
                &trace,
                &compiled,
                &context,
                archive.manifest.runtime.update_hz,
                Some(progress),
            );
            let mut parity = compare_semantics_with_progress(
                &trace,
                &compiled,
                primary,
                &context,
                Some(std::sync::Arc::clone(progress)),
            );
            runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
            // Newer references contain sparse player transforms; older captures
            // retain only visibility and cannot support these additional checks.
            if trace
                .player_render_tracks
                .iter()
                .all(|track| !track.transform_samples.is_empty())
            {
                runtime_modifiers::compare_player_frames(&trace, &compiled, &context, &mut parity);
            }
            assert_eq!(
                progress.checks.load(Ordering::Relaxed),
                parity.checks(),
                "live comparison count must include every comparator"
            );
            assert_eq!(
                progress.failed.load(Ordering::Relaxed),
                parity.checks() - parity.passed(),
                "live failure count must include every comparator"
            );
            eprintln!("\n{}", parity.summary(&entry.title));
            if !parity.is_complete() {
                eprintln!(
                    "parity gaps ({} reported):\n- {}",
                    parity.gaps.len(),
                    parity.gaps.join("\n- ")
                );
            }
            parity.is_complete()
        }));
        use std::sync::atomic::Ordering;
        checks += progress.checks.load(Ordering::Relaxed);
        failed_checks += progress.failed.load(Ordering::Relaxed);
        let succeeded = matches!(outcome, Ok(true));
        if !succeeded {
            failed += 1;
        }
        drop(reporter);
        eprintln!(
            "fixture result: {}",
            if succeeded { "ok" } else { "FAILED" }
        );
    }
    eprintln!(
        "\nresult: {}; {}/{} fixtures passed; {} comparisons passed, {} failed ({} total); elapsed {:.2}s",
        if failed == 0 { "ok" } else { "FAILED" },
        selected.len() - failed,
        selected.len(),
        checks - failed_checks,
        failed_checks,
        checks,
        started.elapsed().as_secs_f64()
    );
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[test]
fn full_song_selector_rejects_missing_and_ambiguous_matches_and_preserves_aliases() {
    let entry = |hash: &str, title: &str| ArchiveEntry {
        title: title.into(),
        source_simfile: format!("{title}/chart.ssc"),
        harness_version: "0.1.0".into(),
        archive: format!("{hash}.tar.zst"),
        sha256: hash.into(),
        compressed_bytes: 1,
        local_only: false,
        aliases: vec![format!("old-{hash}.tar.zst")],
        retained_version: false,
    };
    let index = ArchiveIndex {
        archive_schema_version: 1,
        hash: "sha256-compressed-archive".into(),
        archives: vec![
            entry("abc123", "Example"),
            entry("abc456", "Example Double"),
        ],
    };
    for selector in [
        "abc123.tar.zst",
        "old-abc123.tar.zst",
        "abc123",
        "example/chart.ssc",
    ] {
        assert!(select_archive(&index, selector).is_ok_and(|entry| entry.sha256 == "abc123"));
    }
    assert!(select_archive(&index, "abc").is_err_and(|matches| matches.len() == 2));
    assert!(select_archive(&index, "nonexistent").is_err_and(|matches| matches.is_empty()));
}

#[test]
fn consolidated_song_lua_references_resolve_and_are_compressed() {
    let references: HashMap<String, String> = serde_json::from_slice(
        &fs::read(archive_root().join("references.json")).expect("reference index"),
    )
    .unwrap();
    assert!(!references.is_empty());
    let index = archive_index();
    for (alias, filename) in references {
        let original = Path::new(env!("CARGO_MANIFEST_DIR")).join(&alias);
        let resolved = reference_path(&original);
        assert_eq!(resolved, archive_root().join(&filename), "{alias}");
        if !resolved.is_file() {
            assert!(
                index
                    .archives
                    .iter()
                    .any(|entry| entry.archive == filename && entry.local_only),
                "{filename}"
            );
            continue;
        }
        if !filename.ends_with(".manifest.json") {
            assert!(filename.ends_with(".zst"));
        }
    }
}

#[test]
#[ignore = "explicit full-corpus compile, composition, and exact semantic/render audit"]
fn whole_song_archives_compile_compose_and_match_native_trace() {
    let index = archive_index();
    for entry in selected_archives(&index) {
        assert_eq!(
            run_cli(vec![entry.archive.clone()]),
            std::process::ExitCode::SUCCESS
        );
    }
}
