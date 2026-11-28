use deadsync_simfile::cache::{SerializableSongData, load_song_cache_file, write_song_cache_file};
use deadsync_simfile::song::{ParseSongOptions, parse_song_file};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    simfile: PathBuf,
    cache: PathBuf,
    music: PathBuf,
    song: SerializableSongData,
}

impl Fixture {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-cache-music-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let simfile = root.join("song.sm");
        let cache = root.join("cache.bin");
        let music = root.join("song.ogg");
        fs::write(&music, b"music").unwrap();
        fs::write(&simfile, "#TITLE:Music validation;\n#MUSIC:song.ogg;\n#BPMS:0=120;\n#NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n").unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let mut song = parse_song_file(&simfile, &options, |_| 10.0).unwrap();
        song.charts = vec![song.charts[0].clone(); 6];
        Self {
            root,
            simfile,
            cache,
            music,
            song,
        }
    }

    fn write(&self) {
        write_song_cache_file(&self.cache, &self.song, 0.0).unwrap();
    }

    fn is_valid(&self) -> bool {
        load_song_cache_file(&self.simfile, &self.cache, false).is_some()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn shared_and_overridden_music_are_revalidated_between_cache_loads() {
    let mut fixture = Fixture::new();
    assert!(
        fixture
            .song
            .charts
            .iter()
            .all(|chart| chart.music_path == fixture.song.music_path)
    );
    fixture.write();
    assert!(fixture.is_valid());

    fs::remove_file(&fixture.music).unwrap();
    assert!(
        !fixture.is_valid(),
        "shared music must still exist on every load"
    );
    fs::write(&fixture.music, b"restored").unwrap();
    assert!(fixture.is_valid());

    let override_path = fixture.root.join("override.ogg");
    fs::write(&override_path, b"override").unwrap();
    fixture.song.charts[2].music_path = Some(override_path.to_string_lossy().into_owned());
    fixture.write();
    assert!(fixture.is_valid());
    fs::remove_file(&override_path).unwrap();
    assert!(
        !fixture.is_valid(),
        "distinct chart music must still be checked"
    );
    fs::write(&override_path, b"restored").unwrap();
    assert!(fixture.is_valid());
}

#[test]
fn cached_music_validation_preserves_optional_empty_and_directory_paths() {
    let mut fixture = Fixture::new();
    let valid = Some(fixture.music.to_string_lossy().into_owned());
    let missing = Some(
        fixture
            .root
            .join("missing.ogg")
            .to_string_lossy()
            .into_owned(),
    );
    let directory = Some(fixture.root.to_string_lossy().into_owned());
    let padded = Some(format!(" \t{}\n", fixture.music.display()));
    for (song_path, chart_path, expected) in [
        (None, None, true),
        (valid.clone(), None, true),
        (None, valid.clone(), true),
        (valid.clone(), valid.clone(), true),
        (valid.clone(), padded, true),
        (Some(String::new()), Some(String::new()), false),
        (Some(" \t\n".into()), Some(" \t\n".into()), false),
        (valid.clone(), Some(String::new()), false),
        (Some(String::new()), valid.clone(), false),
        (missing.clone(), missing.clone(), false),
        (valid.clone(), missing.clone(), false),
        (missing, valid.clone(), false),
        (directory.clone(), directory, false),
    ] {
        fixture.song.music_path = song_path.clone();
        for chart in &mut fixture.song.charts {
            chart.music_path = chart_path.clone();
        }
        fixture.write();
        assert_eq!(
            fixture.is_valid(),
            expected,
            "song={song_path:?}, chart={chart_path:?}"
        );
    }
}
