#[path = "support/song_fixture.rs"]
mod song_fixture;

#[path = "support/paths.rs"]
mod paths;

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
    file1: String,
    file2: String,
    rate: f32,
    effect: String,
    transition: String,
    color1: String,
    color2: String,
}

#[derive(Debug, PartialEq, Eq)]
struct LuaChange {
    beat_bits: u32,
    path: String,
}


fn fixture_root() -> PathBuf {
    paths::init();
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

// Layer discovery needs filenames and BG/FG records, not the original music,
// notes or authored scripts. Preserve the native-captured inputs and compare
// discovery against the independently recorded native Lua-entry paths.
fn write_discovery_fixture(root: &Path, entry: &ManifestSimfile, fixture: &Fixture) -> PathBuf {
    let simfile = root.join(&entry.simfile);
    fs::create_dir_all(simfile.parent().unwrap()).unwrap();
    for change in &fixture.changes {
        if let Some(path) = &change.lua_entry {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "return Def.ActorFrame {}\n").unwrap();
        }
    }
    let mut text = String::from("#TITLE:Native layer discovery;\n#BPMS:0=120;\n");
    for (layer, tag) in [
        ("background1", "BGCHANGES"),
        ("background2", "BGCHANGES2"),
        ("foreground", "FGCHANGES"),
    ] {
        let changes = fixture
            .changes
            .iter()
            .filter(|change| change.layer == layer)
            .map(|change| {
                format!(
                    "{}={}={}={}={}={}={}={}={}={}={}",
                    change.start_beat,
                    change.file1,
                    change.rate,
                    0,
                    0,
                    0,
                    change.effect,
                    change.file2,
                    change.transition,
                    change.color1,
                    change.color2
                )
            })
            .collect::<Vec<_>>();
        text.push_str(&format!("#{tag}:{};\n", changes.join(",")));
    }
    if simfile.extension().and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ssc")) {
        text.push_str("#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n1000\n0000\n0000\n0000;\n");
    } else {
        text.push_str("#NOTES:dance-single:fixture:Challenge:1:0,0,0,0,0:\n1000\n0000\n0000\n0000;\n");
    }
    fs::write(&simfile, text).unwrap();
    simfile
}

#[test]
fn itgmania_song_lua_layer_discovery_matches_deadsync() {
    let generated = tempfile::tempdir().unwrap();
    let corpus_root = generated.path().canonicalize().unwrap();
    let fixtures = fixture_root();
    let mut failures = Vec::new();

    for entry in read_manifest().simfiles {
        let fixture: Fixture = serde_json::from_slice(
            &fs::read(fixtures.join(&entry.fixture))
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", entry.fixture)),
        )
        .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", entry.fixture));
        let simfile = write_discovery_fixture(&corpus_root, &entry, &fixture);
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
    paths::init();
    let mut compiled_sessions = HashSet::new();
    let mut failures = Vec::new();
    let mut compiled_entries = 0;

    for entry in read_manifest().simfiles {
        let fixture = song_fixture::from_source(&entry.simfile);
        let simfile = fixture.simfile.clone();
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
            .map(|path| relative_path(simfile.parent().unwrap(), path))
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
#[ignore = "long song-corpus compilation, including ten seconds of every archived scene"]
fn lua_song_corpus_smoke_compiles_in_deadsync() {
    assert_corpus_compiles(Some(10.0));
}

#[test]
#[ignore = "full-song sampling is intentionally an explicit, long-running parity check"]
fn lua_song_corpus_fully_compiles_in_deadsync() {
    assert_corpus_compiles(None);
}

#[test]
#[ignore = "full-song compile checks for charts without complete reference traces"]
fn reported_song_lua_failures_compile() {
    paths::init();
    let filter = std::env::var("SONG_LUA_COMPILE_FILTER").unwrap_or_default();
    let limit = std::env::var("SONG_LUA_COMPILE_SECONDS")
        .ok()
        .map(|value| value.parse::<f32>().expect("numeric compile duration"));
    let mut failures = Vec::new();
    let mut checked = 0;
    for relative in [
        "Apollo/Apollo.ssc",
        "Sharkmode [Ky_Dash]/Sharkmode.ssc",
        "mawaru7/mawaru7.sm",
        "mawaru8/mawaru8.sm",
        "mawaru6/mawaru6.sm",
        "mawaru4/mawaru4.sm",
        "Igaku/Igaku.ssc",
        "flowers/flowers.ssc",
        "Do the Mario/DoTheMario.ssc",
        "Megalovania SM5 [TaroNuke]/megalovania.ssc",
    ] {
        if !relative.contains(&filter) {
            continue;
        }
        let fixture = song_fixture::from_source(relative);
        let simfile = fixture.simfile.clone();
        let song = parse_song(&simfile);
        let entries = song
            .background_lua_changes
            .iter()
            .map(|change| change.path.as_path())
            .chain(
                song.foreground_lua_changes
                    .iter()
                    .map(|change| change.path.as_path()),
            )
            .collect::<Vec<_>>();
        assert!(!entries.is_empty(), "{relative} has no Lua entries");
        let primary = song
            .foreground_lua_changes
            .iter()
            .position(|change| change.start_beat <= 0.0)
            .map_or(0, |index| song.background_lua_changes.len() + index);
        let mut context = SongLuaCompileContext::new(
            simfile.parent().expect("song directory"),
            song.title.clone(),
        );
        context.song_display_bpms = [song.min_bpm as f32, song.max_bpm as f32];
        context.song_timing_bpms = deadsync_song_lua::parse_song_timing_bpms(&song.normalized_bpms);
        context.music_length_seconds = limit.map_or(song.precise_last_second(), |limit| {
            limit.min(song.precise_last_second())
        });
        context.video_renderers = "opengl,software".to_owned();
        for player in &mut context.players {
            player.enabled = true;
            player.difficulty = SongLuaDifficulty::Challenge;
        }
        checked += 1;
        match compile_song_lua_layers(&entries, primary, &context) {
            Ok(compiled) => {
                assert_eq!(compiled.len(), entries.len());
                eprintln!("{relative}: COMPILED ({} layers)", compiled.len());
            }
            Err(error) => {
                eprintln!("{relative}: FAILED: {error}");
                failures.push(format!("{relative}: {error}"));
            }
        }
    }
    assert!(checked > 0, "no charts matched {filter:?}");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
