use deadsync_assets::song_lua::{
    SongLuaCompileContext, SongLuaDifficulty, SongLuaPlayerContext, SongLuaSpeedMod,
    compile_song_lua_layers,
};
use deadsync_simfile::song::{ParseSongOptions, parse_song_meta_file};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Manifest {
    simfiles: Vec<ManifestSimfile>,
}

#[derive(Deserialize)]
struct ManifestSimfile {
    simfile: String,
    fixture: String,
}

#[derive(Deserialize)]
struct Fixture {
    changes: Vec<NativeChange>,
}

#[derive(Deserialize)]
struct NativeChange {
    layer: String,
    start_beat: f32,
    lua_entry: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct LuaChange {
    beat_bits: u32,
    path: String,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("deadsync should have a workspace parent")
        .to_owned()
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/itgmania-song-lua")
}

fn read_manifest() -> Manifest {
    serde_json::from_slice(
        &fs::read(fixture_root().join("_manifest.json"))
            .expect("missing ITGmania song Lua fixture manifest"),
    )
    .expect("invalid ITGmania song Lua fixture manifest")
}

fn relative_path(root: &Path, path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.strip_prefix(root)
        .unwrap_or_else(|_| panic!("{} is outside {}", path.display(), root.display()))
        .to_string_lossy()
        .replace('\\', "/")
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

#[test]
fn itgmania_song_lua_layer_discovery_matches_deadsync() {
    let corpus_root = workspace_root()
        .join("lua-songs")
        .canonicalize()
        .expect("missing workspace lua-songs corpus");
    let fixtures = fixture_root();
    let mut failures = Vec::new();

    for entry in read_manifest().simfiles {
        let fixture: Fixture = serde_json::from_slice(
            &fs::read(fixtures.join(&entry.fixture))
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", entry.fixture)),
        )
        .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", entry.fixture));
        let simfile = corpus_root.join(&entry.simfile);
        let song = parse_song(&simfile);

        let expected_background = fixture
            .changes
            .iter()
            .filter(|change| matches!(change.layer.as_str(), "background1" | "background2"))
            .filter_map(|change| {
                Some(LuaChange {
                    beat_bits: change.start_beat.to_bits(),
                    path: change.lua_entry.clone()?,
                })
            })
            .collect::<Vec<_>>();
        let expected_foreground = fixture
            .changes
            .iter()
            .filter(|change| change.layer == "foreground")
            .filter_map(|change| {
                Some(LuaChange {
                    beat_bits: change.start_beat.to_bits(),
                    path: change.lua_entry.clone()?,
                })
            })
            .collect::<Vec<_>>();
        let actual_background = song
            .background_lua_changes
            .iter()
            .map(|change| LuaChange {
                beat_bits: change.start_beat.to_bits(),
                path: relative_path(&corpus_root, &change.path),
            })
            .collect::<Vec<_>>();
        let actual_foreground = song
            .foreground_lua_changes
            .iter()
            .map(|change| LuaChange {
                beat_bits: change.start_beat.to_bits(),
                path: relative_path(&corpus_root, &change.path),
            })
            .collect::<Vec<_>>();

        if expected_background != actual_background || expected_foreground != actual_foreground {
            failures.push(format!(
                "{}\n  native background: {expected_background:?}\n  deadsync background: {actual_background:?}\n  native foreground: {expected_foreground:?}\n  deadsync foreground: {actual_foreground:?}",
                entry.simfile
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "song Lua layer parity failures:\n{}",
        failures.join("\n")
    );
}

fn compile_corpus(max_seconds: Option<f32>) -> (usize, Vec<String>) {
    let corpus_root = workspace_root()
        .join("lua-songs")
        .canonicalize()
        .expect("missing workspace lua-songs corpus");
    let mut compiled_sessions = HashSet::new();
    let mut failures = Vec::new();
    let mut compiled_entries = 0;

    for entry in read_manifest().simfiles {
        let simfile = corpus_root.join(&entry.simfile);
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
        if paths.is_empty() {
            continue;
        }
        let session_key = paths
            .iter()
            .map(|path| relative_path(&corpus_root, path))
            .collect::<Vec<_>>()
            .join("\n");
        if !compiled_sessions.insert(session_key) {
            continue;
        }

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
        context.music_length_seconds = max_seconds.map_or_else(
            || song.precise_last_second(),
            |limit| limit.min(song.precise_last_second()),
        );
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

        eprintln!("compiling {}", entry.simfile);
        match compile_song_lua_layers(&paths, primary_index, &context) {
            Ok(compiled) => compiled_entries += compiled.len(),
            Err(error) => failures.push(format!("{}: {error}", entry.simfile)),
        }
    }

    (compiled_entries, failures)
}

fn assert_corpus_compiles(max_seconds: Option<f32>) {
    let (compiled_entries, failures) = compile_corpus(max_seconds);

    assert!(compiled_entries > 0, "song Lua corpus contained no entries");
    assert!(
        failures.is_empty(),
        "DeadSync song Lua corpus failures:\n{}",
        failures.join("\n\n")
    );
}

#[test]
#[ignore = "reports the current DeadSync song Lua compatibility gaps"]
fn lua_song_corpus_smoke_compiles_in_deadsync() {
    assert_corpus_compiles(Some(10.0));
}

#[test]
#[ignore = "full-song sampling is intentionally an explicit, long-running parity check"]
fn lua_song_corpus_fully_compiles_in_deadsync() {
    assert_corpus_compiles(None);
}
