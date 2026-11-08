//! Song inputs come only from the committed, content-addressed fixture store.
//! Extract into a temporary directory; never search a developer's song folders.
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub struct SongFixture {
    _temp: tempfile::TempDir,
    pub simfile: PathBuf,
}

fn store() -> PathBuf {
    // Works both for root tests and for crate tests that include this module.
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !root
        .join("tests/fixtures/full_song_lua/index.json")
        .is_file()
    {
        assert!(root.pop(), "workspace fixture store is missing");
    }
    root.join("tests/fixtures/full_song_lua")
}

pub fn index() -> &'static Vec<Value> {
    static INDEX: OnceLock<Vec<Value>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let data: Value =
            serde_json::from_slice(&fs::read(store().join("index.json")).unwrap()).unwrap();
        let committed = include_str!("../fixtures/full_song_lua/committed-archives.txt")
            .lines()
            .collect::<std::collections::HashSet<_>>();
        let mut entries = data["archives"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| committed.contains(entry["archive"].as_str().unwrap()))
            .cloned()
            .collect::<Vec<_>>();
        // Native regression traces also refer to retained captures. The catalog
        // is generated from each committed archive's manifest (not local files).
        let mut retained: Vec<Value> = serde_json::from_str(include_str!(
            "../fixtures/full_song_lua/committed-songs.json"
        ))
        .unwrap();
        retained.sort_by_cached_key(|entry| {
            std::cmp::Reverse(
                entry["harness_version"]
                    .as_str()
                    .unwrap()
                    .split('.')
                    .map(|part| part.parse::<u32>().unwrap())
                    .collect::<Vec<_>>(),
            )
        });
        for entry in retained {
            assert!(committed.contains(entry["archive"].as_str().unwrap()));
            if !entries
                .iter()
                .any(|existing| existing["archive"] == entry["archive"])
            {
                entries.push(entry);
            }
        }
        // Input-only bundles cover retained native traces without claiming
        // that their lengthy full-song replays have been accepted.
        let inputs: Vec<Value> = serde_json::from_str(include_str!(
            "../fixtures/song-inputs/index.json"
        )).unwrap();
        entries.extend(inputs);
        entries
    })
}

pub fn from_source(source: &str) -> SongFixture {
    let entry = index()
        .iter()
        .find(|entry| entry["source_simfile"] == source)
        .unwrap_or_else(|| panic!("no committed song fixture for {source:?}"));
    let temp = tempfile::tempdir().expect("temporary song fixture");
    let archive_path = store().join(entry["archive"].as_str().unwrap());
    if let Some(expected) = entry["sha256"].as_str() {
        use sha2::{Digest, Sha256};
        let actual = Sha256::digest(fs::read(&archive_path).unwrap()).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        assert_eq!(actual, expected, "input bundle checksum differs");
    }
    let input = fs::File::open(archive_path).unwrap();
    let decoder = zstd::stream::read::Decoder::new(input).unwrap();
    let mut archive = tar::Archive::new(decoder);
    for member in archive.entries().unwrap() {
        assert!(
            member.unwrap().unpack_in(temp.path()).unwrap(),
            "fixture member escaped its root"
        );
    }
    let manifest: Value =
        serde_json::from_slice(&fs::read(temp.path().join("manifest.json")).unwrap()).unwrap();
    let relative = Path::new(manifest["chart"]["simfile"].as_str().unwrap());
    assert!(relative.components().all(|part| matches!(part, std::path::Component::Normal(_))),
        "fixture simfile must stay inside its temporary archive root");
    let simfile = temp.path().join(relative);
    assert!(simfile.is_file(), "archive omitted its simfile");
    SongFixture {
        _temp: temp,
        simfile,
    }
}

/// Keep extracted inputs alive for legacy callers that return only a path.
/// The cache is per test process and is released with that process.
pub fn cached(source: &str) -> PathBuf {
    static CACHE: OnceLock<Mutex<HashMap<String, SongFixture>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    cache
        .entry(source.to_owned())
        .or_insert_with(|| from_source(source))
        .simfile
        .clone()
}

pub fn for_trace(title: &str, source: Option<&Path>, simfile: &str) -> PathBuf {
    let normalized = source.map(|path| path.to_string_lossy().replace('\\', "/"));
    if let Some(entry) = index().iter().find(|entry| {
        normalized.as_deref().is_some_and(|source| {
            source == entry["source_simfile"].as_str().unwrap()
                || source.ends_with(&format!("/{}", entry["source_simfile"].as_str().unwrap()))
        })
    }) {
        return cached(entry["source_simfile"].as_str().unwrap());
    }
    let filename = simfile.rsplit(['/', '\\']).next().unwrap();
    let candidates = index()
        .iter()
        .filter(|entry| {
            entry["title"] == title
                && Path::new(entry["source_simfile"].as_str().unwrap())
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case(filename))
        })
        .map(|entry| entry["source_simfile"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        candidates.len(),
        1,
        "no unique committed fixture for {title:?} ({simfile})"
    );
    cached(candidates.first().unwrap())
}
